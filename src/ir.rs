//! Platform-neutral Widget Tree IR v1.
//!
//! Matches `swift/runtime/WidgetIR.swift` wire-for-wire so Rust-compiled
//! snapshots decode losslessly inside the SwiftUI WidgetKit extension.

use std::collections::BTreeMap;

use nsh_core::{Color, Hsla, element::PropEnum, geometry::Points};
use serde::{Deserialize, Serialize};

use crate::config::WidgetFamily;

/// Current wire version of [`WidgetSnapshot`].
pub const WIDGET_IR_VERSION: u32 = 1;

/// Maximum number of IR nodes permitted in a single [`WidgetFamilyTree`].
pub const MAX_WIDGET_NODES: usize = 256;

/// Maximum nesting depth permitted in a single [`WidgetFamilyTree`].
pub const MAX_WIDGET_TREE_DEPTH: usize = 16;

/// Maximum serialized JSON byte size permitted for one [`WidgetSnapshot`].
pub const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;

/// A serialized snapshot for one widget, containing one or more scheduled
/// timeline entries with pre-evaluated trees for each configured family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetSnapshot {
    /// IR protocol version ([`WIDGET_IR_VERSION`]).
    pub version: u32,
    /// Widget identifier.
    pub widget_id: String,
    /// Unix timestamp in milliseconds when this snapshot was generated.
    pub generated_at_ms: i64,
    /// Optional reload policy hint (milliseconds from `generated_at_ms`) for
    /// WidgetKit `TimelineReloadPolicy`. `None` uses `.atEnd` (or `.never`
    /// when `reload_never` is true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_after_ms: Option<i64>,
    /// When `true`, instructs WidgetKit to use `.never` reload policy until
    /// the main app explicitly calls `Widget.reload`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reload_never: bool,
    /// Ordered timeline entries (at least 1).
    pub entries: Vec<WidgetTimelineIrEntry>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Default for WidgetSnapshot {
    fn default() -> Self {
        Self {
            version: WIDGET_IR_VERSION,
            widget_id: String::new(),
            generated_at_ms: 0,
            reload_after_ms: None,
            reload_never: false,
            entries: Vec::new(),
        }
    }
}

impl WidgetSnapshot {
    /// Checks node-count, tree-depth, and serialized byte-size budgets.
    pub fn validate(&self) -> Result<(), WidgetBudgetError> {
        if self.version != WIDGET_IR_VERSION {
            return Err(WidgetBudgetError::UnsupportedVersion(self.version));
        }
        if self.entries.is_empty() {
            return Err(WidgetBudgetError::EmptyTimeline);
        }
        for entry in &self.entries {
            for (&family, tree) in &entry.families {
                let mut count = 0usize;
                let mut max_depth = 0usize;
                count_nodes(&tree.root, 1, &mut count, &mut max_depth);
                if count > MAX_WIDGET_NODES {
                    return Err(WidgetBudgetError::TooManyNodes {
                        family,
                        count,
                        limit: MAX_WIDGET_NODES,
                    });
                }
                if max_depth > MAX_WIDGET_TREE_DEPTH {
                    return Err(WidgetBudgetError::TooDeep {
                        family,
                        depth: max_depth,
                        limit: MAX_WIDGET_TREE_DEPTH,
                    });
                }
            }
        }
        let bytes = serde_json::to_vec(self)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if bytes > MAX_SNAPSHOT_BYTES {
            return Err(WidgetBudgetError::SnapshotTooLarge {
                bytes,
                limit: MAX_SNAPSHOT_BYTES,
            });
        }
        Ok(())
    }
}

fn count_nodes(node: &WidgetNode, depth: usize, count: &mut usize, max_depth: &mut usize) {
    *count += 1;
    if depth > *max_depth {
        *max_depth = depth;
    }
    match node {
        WidgetNode::Box { children, .. } => {
            for child in children {
                count_nodes(child, depth + 1, count, max_depth);
            }
        }
        WidgetNode::Link { child, .. } | WidgetNode::Action { child, .. } => {
            count_nodes(child, depth + 1, count, max_depth);
        }
        WidgetNode::Text { .. }
        | WidgetNode::Date { .. }
        | WidgetNode::Image { .. }
        | WidgetNode::Progress { .. } => {}
    }
}

/// Budget or schema violation found when validating a [`WidgetSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WidgetBudgetError {
    /// Snapshot IR version is not supported.
    UnsupportedVersion(u32),
    /// Timeline contains no entries.
    EmptyTimeline,
    /// Tree for `family` exceeds [`MAX_WIDGET_NODES`].
    TooManyNodes {
        /// Family that exceeded the limit.
        family: WidgetFamily,
        /// Observed node count.
        count: usize,
        /// Configured limit.
        limit: usize,
    },
    /// Tree for `family` exceeds [`MAX_WIDGET_TREE_DEPTH`].
    TooDeep {
        /// Family that exceeded the limit.
        family: WidgetFamily,
        /// Observed depth.
        depth: usize,
        /// Configured limit.
        limit: usize,
    },
    /// Serialized snapshot exceeds [`MAX_SNAPSHOT_BYTES`].
    SnapshotTooLarge {
        /// Observed serialized byte size.
        bytes: usize,
        /// Configured byte limit.
        limit: usize,
    },
}

impl std::fmt::Display for WidgetBudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "unsupported widget IR version {v}"),
            Self::EmptyTimeline => write!(f, "widget timeline must contain at least one entry"),
            Self::TooManyNodes {
                family,
                count,
                limit,
            } => write!(
                f,
                "widget family {:?} has {count} nodes (maximum is {limit})",
                family.as_str()
            ),
            Self::TooDeep {
                family,
                depth,
                limit,
            } => write!(
                f,
                "widget family {:?} has nesting depth {depth} (maximum is {limit})",
                family.as_str()
            ),
            Self::SnapshotTooLarge { bytes, limit } => {
                write!(
                    f,
                    "widget snapshot is {bytes} bytes (maximum is {limit} bytes)"
                )
            }
        }
    }
}

impl std::error::Error for WidgetBudgetError {}

/// One scheduled timeline entry inside a [`WidgetSnapshot`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetTimelineIrEntry {
    /// Unix timestamp in milliseconds when this entry becomes active.
    pub date_ms: i64,
    /// Optional relevance score (`0.0..=1.0`) for Smart Stack ranking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance: Option<f32>,
    /// Pre-evaluated trees keyed by family (`"small"`, `"medium"`, `"large"`).
    pub families: BTreeMap<WidgetFamily, WidgetFamilyTree>,
}

/// Evaluated tree for one [`WidgetFamily`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetFamilyTree {
    /// Optional root deep link URL (SwiftUI `.widgetURL`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Extracted root background lifted to SwiftUI `.containerBackground(for: .widget)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_background: Option<WidgetFill>,
    /// Root IR node.
    pub root: WidgetNode,
}

impl Default for WidgetFamilyTree {
    fn default() -> Self {
        Self {
            url: None,
            container_background: None,
            root: WidgetNode::Box {
                layout: WidgetLayout::default(),
                style: WidgetBoxStyle::default(),
                children: Vec::new(),
            },
        }
    }
}

/// Dual-appearance sRGB color `[r, g, b, a]` in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetColor {
    /// sRGB `[r, g, b, a]` in Light appearance.
    pub light: [f32; 4],
    /// sRGB `[r, g, b, a]` in Dark appearance.
    pub dark: [f32; 4],
}

impl WidgetColor {
    /// Creates a dual-appearance color from two packed `0xRRGGBBAA` colors.
    pub fn from_packed(light: Color, dark: Color) -> Self {
        Self {
            light: packed_to_rgba(light),
            dark: packed_to_rgba(dark),
        }
    }

    /// Creates a dual-appearance color from two [`Hsla`] colors.
    pub fn from_hsla(light: Hsla, dark: Hsla) -> Self {
        let l = light.to_rgb();
        let d = dark.to_rgb();
        Self {
            light: [l.r, l.g, l.b, l.a],
            dark: [d.r, d.g, d.b, d.a],
        }
    }

    /// True if the color is noticeably chromatic (not neutral gray/white/black),
    /// used as a smart default for SwiftUI `.widgetAccentable(true)`.
    pub fn is_chromatic(&self) -> bool {
        let is_sat = |c: [f32; 4]| {
            let max = c[0].max(c[1]).max(c[2]);
            let min = c[0].min(c[1]).min(c[2]);
            c[3] > 0.05 && (max - min) > 0.14
        };
        is_sat(self.light) || is_sat(self.dark)
    }
}

fn packed_to_rgba(color: Color) -> [f32; 4] {
    let (r, g, b, a) = color.to_f64();
    [r as f32, g as f32, b as f32, a as f32]
}

/// Fill paint for widget backgrounds and container backgrounds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum WidgetFill {
    /// Solid dual-appearance color.
    Solid {
        /// Fill color.
        color: WidgetColor,
    },
    /// Linear gradient with angle in degrees (CSS convention: 0° = up, 90° = right, 180° = down).
    LinearGradient {
        /// Angle in degrees.
        angle_deg: f32,
        /// Color stops ordered from 0.0 to 1.0.
        stops: Vec<WidgetGradientStop>,
    },
}

// Flat wire representation avoids `#[serde(tag = "kind")]` buffering into
// `serde::__private::de::Content`, which fails on floats when `serde_json`
// has `arbitrary_precision` enabled in a consumer workspace.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetFillWire {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<WidgetColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    angle_deg: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stops: Option<Vec<WidgetGradientStop>>,
}

impl Serialize for WidgetFill {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Solid { color } => WidgetFillWire {
                kind: "solid".into(),
                color: Some(*color),
                angle_deg: None,
                stops: None,
            },
            Self::LinearGradient { angle_deg, stops } => WidgetFillWire {
                kind: "linearGradient".into(),
                color: None,
                angle_deg: Some(*angle_deg),
                stops: Some(stops.clone()),
            },
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WidgetFill {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WidgetFillWire::deserialize(deserializer)?;
        match wire.kind.as_str() {
            "solid" => Ok(Self::Solid {
                color: wire
                    .color
                    .ok_or_else(|| serde::de::Error::missing_field("color"))?,
            }),
            "linearGradient" => Ok(Self::LinearGradient {
                angle_deg: wire
                    .angle_deg
                    .ok_or_else(|| serde::de::Error::missing_field("angleDeg"))?,
                stops: wire
                    .stops
                    .ok_or_else(|| serde::de::Error::missing_field("stops"))?,
            }),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["solid", "linearGradient"],
            )),
        }
    }
}

/// One stop of a [`WidgetFill::LinearGradient`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetGradientStop {
    /// Dual-appearance stop color.
    pub color: WidgetColor,
    /// Stop position in `0.0..=1.0`.
    pub position: f32,
}

/// Primary layout axis of a [`WidgetNode::Box`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetAxis {
    /// Horizontal stack (`HStack`).
    Row,
    /// Vertical stack (`VStack`).
    #[default]
    Column,
    /// Overlay stack (`ZStack`).
    Stack,
}

/// Distribution along a container's primary axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetMainAlign {
    /// Packed at the start.
    #[default]
    Start,
    /// Centered.
    Center,
    /// Packed at the end.
    End,
    /// Even space between children (`Spacer()` between items).
    SpaceBetween,
    /// Even space around children.
    SpaceAround,
    /// Equal space between and around children.
    SpaceEvenly,
}

/// Alignment along a container's cross axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetCrossAlign {
    /// Aligned to the leading/top edge.
    #[default]
    Start,
    /// Centered on the cross axis.
    Center,
    /// Aligned to the trailing/bottom edge.
    End,
    /// Stretched to fill the cross axis.
    Stretch,
}

/// Four-sided edge insets in points.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetEdges {
    /// Top inset.
    pub top: Points,
    /// Right inset.
    pub right: Points,
    /// Bottom inset.
    pub bottom: Points,
    /// Left inset.
    pub left: Points,
}

/// Layout properties of a widget IR node.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetLayout {
    /// Stack axis (for container nodes).
    #[serde(default)]
    pub axis: WidgetAxis,
    /// Primary-axis alignment.
    #[serde(default)]
    pub main_align: WidgetMainAlign,
    /// Cross-axis alignment.
    #[serde(default)]
    pub cross_align: WidgetCrossAlign,
    /// Gap between adjacent children in points.
    #[serde(default)]
    pub gap: Points,
    /// Inner padding in points.
    #[serde(default)]
    pub padding: WidgetEdges,
    /// Explicit width in points, if constrained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Points>,
    /// Explicit height in points, if constrained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Points>,
    /// Minimum width in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<Points>,
    /// Minimum height in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<Points>,
    /// Maximum width in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<Points>,
    /// Maximum height in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<Points>,
    /// Flex grow factor (> 0 expands to fill remaining space).
    #[serde(default)]
    pub flex_grow: f32,
}

/// Visual box decoration properties of a widget container or image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetBoxStyle {
    /// Background paint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<WidgetFill>,
    /// Border stroke color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_color: Option<WidgetColor>,
    /// Border stroke width in points.
    #[serde(default)]
    pub border_width: Points,
    /// Uniform corner radius in points.
    #[serde(default)]
    pub corner_radius: Points,
    /// Opacity (`0.0..=1.0`).
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Whether children are clipped to the rounded box.
    #[serde(default)]
    pub clip: bool,
    /// Whether this box participates in the macOS Tinted accent group.
    #[serde(default)]
    pub accentable: bool,
}

fn default_opacity() -> f32 {
    1.0
}

impl Default for WidgetBoxStyle {
    fn default() -> Self {
        Self {
            background: None,
            border_color: None,
            border_width: Points(0.0),
            corner_radius: Points(0.0),
            opacity: 1.0,
            clip: false,
            accentable: false,
        }
    }
}

/// Horizontal text alignment inside a widget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetTextAlign {
    /// Leading edge (left in LTR).
    #[default]
    Leading,
    /// Centered.
    Center,
    /// Trailing edge (right in LTR).
    Trailing,
}

/// Resolved text styling for a text run or live date node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetTextStyle {
    /// Font size in points.
    pub font_size: Points,
    /// Numeric font weight (`100`..=`900`, default `400`).
    pub font_weight: u16,
    /// Italic style.
    #[serde(default)]
    pub italic: bool,
    /// Monospaced font design.
    #[serde(default)]
    pub monospaced: bool,
    /// Dual-appearance text foreground color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<WidgetColor>,
    /// Underline decoration.
    #[serde(default)]
    pub underline: bool,
    /// Strikethrough decoration.
    #[serde(default)]
    pub strikethrough: bool,
    /// Tracking / letter spacing in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<Points>,
}

impl Default for WidgetTextStyle {
    fn default() -> Self {
        Self {
            font_size: Points(14.0),
            font_weight: 400,
            italic: false,
            monospaced: false,
            color: None,
            underline: false,
            strikethrough: false,
            letter_spacing: None,
        }
    }
}

/// One styled text span inside [`WidgetNode::Text`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetTextRun {
    /// Plain text characters of this span.
    pub text: String,
    /// Resolved typography and color.
    pub style: WidgetTextStyle,
}

/// How [`WidgetNode::Date`] formats its live OS-driven timestamp.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetDateMode {
    /// Relative wording (`"2m ago"`, `"in 5m"`).
    #[default]
    Relative,
    /// Live counting timer (`"04:12"`).
    Timer,
    /// Signed offset (`"+3m"`, `"-12s"`).
    Offset,
    /// Localized clock time (`"14:30"`).
    Time,
    /// Localized calendar date (`"Nov 22, 2026"`).
    Date,
}

/// Image source inside a widget IR tree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WidgetImageSource {
    /// App asset copied into the widget bundle / App Group (`assets/<path>`).
    Asset {
        /// Relative asset path.
        path: String,
    },
    /// File path inside the shared App Group container.
    File {
        /// Relative file path inside the container.
        path: String,
    },
    /// Platform vector symbol (SF Symbol name on macOS/iOS).
    Symbol {
        /// Symbol identifier (e.g. `"arrow.clockwise"`).
        name: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetImageSourceWire {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Serialize for WidgetImageSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Asset { path } => WidgetImageSourceWire {
                kind: "asset".into(),
                path: Some(path.clone()),
                name: None,
            },
            Self::File { path } => WidgetImageSourceWire {
                kind: "file".into(),
                path: Some(path.clone()),
                name: None,
            },
            Self::Symbol { name } => WidgetImageSourceWire {
                kind: "symbol".into(),
                path: None,
                name: Some(name.clone()),
            },
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WidgetImageSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WidgetImageSourceWire::deserialize(deserializer)?;
        match wire.kind.as_str() {
            "asset" => Ok(Self::Asset {
                path: wire
                    .path
                    .ok_or_else(|| serde::de::Error::missing_field("path"))?,
            }),
            "file" => Ok(Self::File {
                path: wire
                    .path
                    .ok_or_else(|| serde::de::Error::missing_field("path"))?,
            }),
            "symbol" => Ok(Self::Symbol {
                name: wire
                    .name
                    .ok_or_else(|| serde::de::Error::missing_field("name"))?,
            }),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["asset", "file", "symbol"],
            )),
        }
    }
}

/// How an image fits within its widget box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetResizeMode {
    /// Scale uniformly so the entire image fits inside the box.
    #[default]
    Contain,
    /// Scale uniformly to fill the box, cropping overflow.
    Cover,
    /// Stretch independently along both axes.
    Stretch,
    /// Draw at intrinsic size centered in the box.
    Center,
}

/// How an image renders in macOS 15+ Tinted (`accented`) mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetAccentedImageMode {
    /// Keep original full-color pixels even in Tinted mode.
    FullColor,
    /// Tint the image with the system accent color.
    Accented,
    /// Desaturate and tint tastefully (macOS default for photos/icons).
    #[default]
    Desaturated,
    /// Render as a monochrome template mask.
    Template,
}

/// A platform-neutral node in the evaluated Widget Tree IR.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum WidgetNode {
    /// Layout container (`VStack`, `HStack`, or `ZStack`).
    Box {
        /// Stack layout rules.
        layout: WidgetLayout,
        /// Visual box decoration.
        style: WidgetBoxStyle,
        /// Child IR nodes.
        children: Vec<WidgetNode>,
    },
    /// Rich or plain text paragraph.
    Text {
        /// Styled inline text runs.
        runs: Vec<WidgetTextRun>,
        /// Horizontal alignment.
        align: WidgetTextAlign,
        /// Maximum number of displayed lines (`None`: unlimited).
        line_limit: Option<u32>,
        /// Participate in the Tinted accent group.
        accentable: bool,
        /// Node layout constraints.
        layout: WidgetLayout,
    },
    /// Live OS-driven date or countdown text.
    Date {
        /// Target Unix timestamp in milliseconds.
        timestamp_ms: i64,
        /// Formatting mode.
        mode: WidgetDateMode,
        /// Resolved text style.
        style: WidgetTextStyle,
        /// Participate in the Tinted accent group.
        accentable: bool,
        /// Node layout constraints.
        layout: WidgetLayout,
    },
    /// Static asset, App Group image, or platform symbol.
    Image {
        /// Image source.
        source: WidgetImageSource,
        /// Content fit mode.
        resize_mode: WidgetResizeMode,
        /// Optional dual-appearance tint color.
        tint: Option<WidgetColor>,
        /// Rendering behavior in Tinted mode.
        accented_mode: WidgetAccentedImageMode,
        /// Box decoration (radius, border, opacity).
        style: WidgetBoxStyle,
        /// Node layout constraints.
        layout: WidgetLayout,
    },
    /// Linear determinate progress bar.
    Progress {
        /// Progress fraction in `0.0..=1.0`.
        value: f32,
        /// Optional dual-appearance track background color.
        track_color: Option<WidgetColor>,
        /// Optional dual-appearance indicator fill color.
        indicator_color: Option<WidgetColor>,
        /// Optional corner radius in points.
        corner_radius: Option<Points>,
        /// Node layout constraints.
        layout: WidgetLayout,
    },
    /// Sub-region deep link (`SwiftUI Link(destination:)`).
    Link {
        /// Target deep link URL.
        href: String,
        /// Optional VoiceOver / accessibility label.
        accessibility_label: Option<String>,
        /// Visual content of the link.
        child: Box<WidgetNode>,
    },
    /// Interactive desktop button (`SwiftUI Button(intent:)`).
    Action {
        /// Action identifier dispatched to `Widget.on("action", ...)`.
        action: String,
        /// Optional serialized JSON payload attached to the button.
        payload_json: Option<String>,
        /// Optional serialized JSON patch applied optimistically when offline.
        patch_json: Option<String>,
        /// Optional VoiceOver / accessibility label.
        accessibility_label: Option<String>,
        /// Visual content of the button.
        child: Box<WidgetNode>,
    },
}

impl Serialize for WidgetNode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct BoxOut<'a> {
            kind: &'static str,
            layout: &'a WidgetLayout,
            style: &'a WidgetBoxStyle,
            children: &'a [WidgetNode],
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct TextOut<'a> {
            kind: &'static str,
            runs: &'a [WidgetTextRun],
            align: WidgetTextAlign,
            #[serde(skip_serializing_if = "Option::is_none")]
            line_limit: Option<u32>,
            accentable: bool,
            layout: &'a WidgetLayout,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct DateOut<'a> {
            kind: &'static str,
            timestamp_ms: i64,
            mode: WidgetDateMode,
            style: &'a WidgetTextStyle,
            accentable: bool,
            layout: &'a WidgetLayout,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ImageOut<'a> {
            kind: &'static str,
            source: &'a WidgetImageSource,
            resize_mode: WidgetResizeMode,
            #[serde(skip_serializing_if = "Option::is_none")]
            tint: Option<WidgetColor>,
            accented_mode: WidgetAccentedImageMode,
            style: &'a WidgetBoxStyle,
            layout: &'a WidgetLayout,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ProgressOut<'a> {
            kind: &'static str,
            value: f32,
            #[serde(skip_serializing_if = "Option::is_none")]
            track_color: Option<WidgetColor>,
            #[serde(skip_serializing_if = "Option::is_none")]
            indicator_color: Option<WidgetColor>,
            #[serde(skip_serializing_if = "Option::is_none")]
            corner_radius: Option<Points>,
            layout: &'a WidgetLayout,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct LinkOut<'a> {
            kind: &'static str,
            href: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            accessibility_label: Option<&'a str>,
            child: &'a WidgetNode,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ActionOut<'a> {
            kind: &'static str,
            action: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            payload_json: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            patch_json: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            accessibility_label: Option<&'a str>,
            child: &'a WidgetNode,
        }
        match self {
            Self::Box {
                layout,
                style,
                children,
            } => BoxOut {
                kind: "box",
                layout,
                style,
                children,
            }
            .serialize(serializer),
            Self::Text {
                runs,
                align,
                line_limit,
                accentable,
                layout,
            } => TextOut {
                kind: "text",
                runs,
                align: *align,
                line_limit: *line_limit,
                accentable: *accentable,
                layout,
            }
            .serialize(serializer),
            Self::Date {
                timestamp_ms,
                mode,
                style,
                accentable,
                layout,
            } => DateOut {
                kind: "date",
                timestamp_ms: *timestamp_ms,
                mode: *mode,
                style,
                accentable: *accentable,
                layout,
            }
            .serialize(serializer),
            Self::Image {
                source,
                resize_mode,
                tint,
                accented_mode,
                style,
                layout,
            } => ImageOut {
                kind: "image",
                source,
                resize_mode: *resize_mode,
                tint: *tint,
                accented_mode: *accented_mode,
                style,
                layout,
            }
            .serialize(serializer),
            Self::Progress {
                value,
                track_color,
                indicator_color,
                corner_radius,
                layout,
            } => ProgressOut {
                kind: "progress",
                value: *value,
                track_color: *track_color,
                indicator_color: *indicator_color,
                corner_radius: *corner_radius,
                layout,
            }
            .serialize(serializer),
            Self::Link {
                href,
                accessibility_label,
                child,
            } => LinkOut {
                kind: "link",
                href,
                accessibility_label: accessibility_label.as_deref(),
                child,
            }
            .serialize(serializer),
            Self::Action {
                action,
                payload_json,
                patch_json,
                accessibility_label,
                child,
            } => ActionOut {
                kind: "action",
                action,
                payload_json: payload_json.as_deref(),
                patch_json: patch_json.as_deref(),
                accessibility_label: accessibility_label.as_deref(),
                child,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for WidgetNode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut raw = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let kind = raw
            .remove("kind")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::missing_field("kind"))?;
        let obj = serde_json::Value::Object(raw);
        let err = |e: serde_json::Error| serde::de::Error::custom(e.to_string());
        match kind.as_str() {
            "box" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct BoxIn {
                    #[serde(default)]
                    layout: WidgetLayout,
                    #[serde(default)]
                    style: WidgetBoxStyle,
                    #[serde(default)]
                    children: Vec<WidgetNode>,
                }
                let v: BoxIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Box {
                    layout: v.layout,
                    style: v.style,
                    children: v.children,
                })
            }
            "text" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct TextIn {
                    #[serde(default)]
                    runs: Vec<WidgetTextRun>,
                    #[serde(default)]
                    align: WidgetTextAlign,
                    #[serde(default)]
                    line_limit: Option<u32>,
                    #[serde(default)]
                    accentable: bool,
                    #[serde(default)]
                    layout: WidgetLayout,
                }
                let v: TextIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Text {
                    runs: v.runs,
                    align: v.align,
                    line_limit: v.line_limit,
                    accentable: v.accentable,
                    layout: v.layout,
                })
            }
            "date" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct DateIn {
                    timestamp_ms: i64,
                    #[serde(default)]
                    mode: WidgetDateMode,
                    #[serde(default)]
                    style: WidgetTextStyle,
                    #[serde(default)]
                    accentable: bool,
                    #[serde(default)]
                    layout: WidgetLayout,
                }
                let v: DateIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Date {
                    timestamp_ms: v.timestamp_ms,
                    mode: v.mode,
                    style: v.style,
                    accentable: v.accentable,
                    layout: v.layout,
                })
            }
            "image" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ImageIn {
                    source: WidgetImageSource,
                    #[serde(default)]
                    resize_mode: WidgetResizeMode,
                    #[serde(default)]
                    tint: Option<WidgetColor>,
                    #[serde(default)]
                    accented_mode: WidgetAccentedImageMode,
                    #[serde(default)]
                    style: WidgetBoxStyle,
                    #[serde(default)]
                    layout: WidgetLayout,
                }
                let v: ImageIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Image {
                    source: v.source,
                    resize_mode: v.resize_mode,
                    tint: v.tint,
                    accented_mode: v.accented_mode,
                    style: v.style,
                    layout: v.layout,
                })
            }
            "progress" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ProgressIn {
                    value: f32,
                    #[serde(default)]
                    track_color: Option<WidgetColor>,
                    #[serde(default)]
                    indicator_color: Option<WidgetColor>,
                    #[serde(default)]
                    corner_radius: Option<Points>,
                    #[serde(default)]
                    layout: WidgetLayout,
                }
                let v: ProgressIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Progress {
                    value: v.value,
                    track_color: v.track_color,
                    indicator_color: v.indicator_color,
                    corner_radius: v.corner_radius,
                    layout: v.layout,
                })
            }
            "link" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct LinkIn {
                    href: String,
                    #[serde(default)]
                    accessibility_label: Option<String>,
                    child: Box<WidgetNode>,
                }
                let v: LinkIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Link {
                    href: v.href,
                    accessibility_label: v.accessibility_label,
                    child: v.child,
                })
            }
            "action" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ActionIn {
                    action: String,
                    #[serde(default)]
                    payload_json: Option<String>,
                    #[serde(default)]
                    patch_json: Option<String>,
                    #[serde(default)]
                    accessibility_label: Option<String>,
                    child: Box<WidgetNode>,
                }
                let v: ActionIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Action {
                    action: v.action,
                    payload_json: v.payload_json,
                    patch_json: v.patch_json,
                    accessibility_label: v.accessibility_label,
                    child: v.child,
                })
            }
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["box", "text", "date", "image", "progress", "link", "action"],
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> WidgetSnapshot {
        let mut families = BTreeMap::new();
        families.insert(
            WidgetFamily::Small,
            WidgetFamilyTree {
                url: Some("pulse://symbol/AAPL".into()),
                container_background: Some(WidgetFill::Solid {
                    color: WidgetColor {
                        light: [1.0, 1.0, 1.0, 1.0],
                        dark: [0.1, 0.1, 0.12, 1.0],
                    },
                }),
                root: WidgetNode::Box {
                    layout: WidgetLayout {
                        axis: WidgetAxis::Column,
                        main_align: WidgetMainAlign::SpaceBetween,
                        cross_align: WidgetCrossAlign::Stretch,
                        gap: Points(6.0),
                        padding: WidgetEdges {
                            top: Points(12.0),
                            right: Points(12.0),
                            bottom: Points(12.0),
                            left: Points(12.0),
                        },
                        flex_grow: 1.0,
                        ..WidgetLayout::default()
                    },
                    style: WidgetBoxStyle::default(),
                    children: vec![
                        WidgetNode::Text {
                            runs: vec![WidgetTextRun {
                                text: "AAPL".into(),
                                style: WidgetTextStyle {
                                    font_size: Points(14.0),
                                    font_weight: 600,
                                    color: Some(WidgetColor {
                                        light: [0.09, 0.09, 0.11, 1.0],
                                        dark: [0.98, 0.98, 0.99, 1.0],
                                    }),
                                    ..WidgetTextStyle::default()
                                },
                            }],
                            align: WidgetTextAlign::Leading,
                            line_limit: Some(1),
                            accentable: false,
                            layout: WidgetLayout::default(),
                        },
                        WidgetNode::Progress {
                            value: 0.72,
                            track_color: Some(WidgetColor {
                                light: [0.9, 0.9, 0.92, 1.0],
                                dark: [0.22, 0.22, 0.25, 1.0],
                            }),
                            indicator_color: Some(WidgetColor {
                                light: [0.06, 0.72, 0.5, 1.0],
                                dark: [0.2, 0.83, 0.6, 1.0],
                            }),
                            corner_radius: Some(Points(4.0)),
                            layout: WidgetLayout {
                                height: Some(Points(6.0)),
                                ..WidgetLayout::default()
                            },
                        },
                        WidgetNode::Action {
                            action: "refresh".into(),
                            payload_json: Some(r#"{"symbol":"AAPL"}"#.into()),
                            patch_json: None,
                            accessibility_label: Some("Refresh quote".into()),
                            child: Box::new(WidgetNode::Date {
                                timestamp_ms: 1_795_280_000_000,
                                mode: WidgetDateMode::Relative,
                                style: WidgetTextStyle {
                                    font_size: Points(11.0),
                                    ..WidgetTextStyle::default()
                                },
                                accentable: false,
                                layout: WidgetLayout::default(),
                            }),
                        },
                    ],
                },
            },
        );
        WidgetSnapshot {
            version: WIDGET_IR_VERSION,
            widget_id: "portfolio".into(),
            generated_at_ms: 1_795_280_000_000,
            reload_after_ms: Some(900_000),
            reload_never: false,
            entries: vec![WidgetTimelineIrEntry {
                date_ms: 1_795_280_000_000,
                relevance: Some(0.8),
                families,
            }],
        }
    }

    #[test]
    fn widget_snapshot_json_round_trip_and_validation() {
        let snapshot = sample_snapshot();
        snapshot.validate().unwrap();
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let decoded: WidgetSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, snapshot);
        assert!(json.contains("\"kind\": \"box\""));
        assert!(json.contains("\"kind\": \"progress\""));
        assert!(json.contains("\"kind\": \"action\""));
        assert!(json.contains("\"kind\": \"date\""));
    }

    #[test]
    fn enforces_node_count_and_depth_budgets() {
        let mut node = WidgetNode::Text {
            runs: Vec::new(),
            align: WidgetTextAlign::Leading,
            line_limit: None,
            accentable: false,
            layout: WidgetLayout::default(),
        };
        for _ in 0..MAX_WIDGET_TREE_DEPTH {
            node = WidgetNode::Box {
                layout: WidgetLayout::default(),
                style: WidgetBoxStyle::default(),
                children: vec![node],
            };
        }
        let mut snap = sample_snapshot();
        snap.entries[0]
            .families
            .get_mut(&WidgetFamily::Small)
            .unwrap()
            .root = node;
        assert!(matches!(
            snap.validate(),
            Err(WidgetBudgetError::TooDeep { .. })
        ));

        let children = (0..=MAX_WIDGET_NODES)
            .map(|_| WidgetNode::Text {
                runs: Vec::new(),
                align: WidgetTextAlign::Leading,
                line_limit: None,
                accentable: false,
                layout: WidgetLayout::default(),
            })
            .collect();
        snap.entries[0]
            .families
            .get_mut(&WidgetFamily::Small)
            .unwrap()
            .root = WidgetNode::Box {
            layout: WidgetLayout::default(),
            style: WidgetBoxStyle::default(),
            children,
        };
        assert!(matches!(
            snap.validate(),
            Err(WidgetBudgetError::TooManyNodes { .. })
        ));
    }
}
