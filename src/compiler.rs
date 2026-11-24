//! Dual-appearance Tailwind v4 + theme token compiler from declarative raw
//! widget trees (`RawWidgetSnapshot`) to [`WidgetSnapshot`] IR.
//!
//! Uses `nsh_core::tailwind::{parse, Theme, ClassList, Context}` directly so
//! widget styling resolves colors, typography, spacing, borders, and gradients
//! identically to Native Shell views in both Light and Dark appearances without
//! needing access to private `Core` shadow-tree internals.

use std::collections::BTreeMap;

use nsh_core::{
    Color, FamilyName, FontFamily, JsonValue, ResizeMode, Style, TextAlign, ViewStyle,
    element::{AdaptiveColor, FONT_MONO, FONT_SANS, Tokens},
    geometry::{Points, Size},
    tailwind::{
        ClassEnvironment, ClassProps, Context as TwContext, Platform, Pointer, Theme, parse,
    },
};
use serde::Deserialize;
use taffy::{AlignItems, FlexDirection, JustifyContent, LengthPercentage, LengthPercentageAuto};

use crate::{
    config::WidgetFamily,
    ir::{
        WIDGET_IR_VERSION, WidgetAccentedImageMode, WidgetAxis, WidgetBoxStyle, WidgetColor,
        WidgetCrossAlign, WidgetDateMode, WidgetEdges, WidgetFamilyTree, WidgetFill,
        WidgetGradientStop, WidgetImageSource, WidgetLayout, WidgetMainAlign, WidgetNode,
        WidgetResizeMode, WidgetSnapshot, WidgetTextAlign, WidgetTextRun, WidgetTextStyle,
        WidgetTimelineIrEntry,
    },
};

/// Raw declarative snapshot sent from JS (`Widget.update` / `Widget.setTimeline`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWidgetSnapshot {
    /// Widget id.
    pub widget_id: String,
    /// Optional generation timestamp in ms (defaults to current system time).
    #[serde(default)]
    pub generated_at_ms: Option<i64>,
    /// Optional reload policy (`"atEnd"`, `"never"`, or `{ "after": ms }`).
    #[serde(default)]
    pub reload_policy: Option<JsonValue>,
    /// Timeline entries to compile.
    pub entries: Vec<RawWidgetTimelineEntry>,
}

/// One timeline entry in [`RawWidgetSnapshot`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWidgetTimelineEntry {
    /// Unix timestamp in ms when this entry becomes active.
    pub date_ms: i64,
    /// Optional Smart Stack relevance (`0.0..=1.0`).
    #[serde(default)]
    pub relevance: Option<f32>,
    /// Unevaluated declarative trees per family.
    pub families: BTreeMap<WidgetFamily, RawWidgetTree>,
}

/// One family's declarative tree before dual-appearance compilation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWidgetTree {
    /// Optional root deep-link URL (SwiftUI `.widgetURL`).
    #[serde(default)]
    pub url: Option<String>,
    /// Root declarative node.
    pub root: RawWidgetNode,
}

/// Optional widget visual hints attached to a declarative node.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWidgetHints {
    /// Explicitly include or exclude from macOS Tinted accent group.
    #[serde(default)]
    pub accentable: Option<bool>,
    /// Override how an image renders in macOS 15+ Tinted mode.
    #[serde(default)]
    pub accented_rendering_mode: Option<WidgetAccentedImageMode>,
    /// Control whether a root box's background lifts to `.containerBackground`.
    #[serde(default)]
    pub container_background: Option<bool>,
}

/// Optional inline style overrides on top of Tailwind `class`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWidgetStyle {
    #[serde(default)]
    pub flex_direction: Option<String>,
    #[serde(default)]
    pub justify_content: Option<String>,
    #[serde(default)]
    pub align_items: Option<String>,
    #[serde(default)]
    pub gap: Option<f32>,
    #[serde(default)]
    pub padding: Option<f32>,
    #[serde(default)]
    pub padding_horizontal: Option<f32>,
    #[serde(default)]
    pub padding_vertical: Option<f32>,
    #[serde(default)]
    pub padding_top: Option<f32>,
    #[serde(default)]
    pub padding_right: Option<f32>,
    #[serde(default)]
    pub padding_bottom: Option<f32>,
    #[serde(default)]
    pub padding_left: Option<f32>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub min_width: Option<f32>,
    #[serde(default)]
    pub min_height: Option<f32>,
    #[serde(default)]
    pub max_width: Option<f32>,
    #[serde(default)]
    pub max_height: Option<f32>,
    #[serde(default)]
    pub flex_grow: Option<f32>,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_width: Option<f32>,
    #[serde(default)]
    pub border_radius: Option<f32>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub overflow: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub font_weight: Option<u16>,
    #[serde(default)]
    pub font_style: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub letter_spacing: Option<f32>,
    #[serde(default)]
    pub text_align: Option<String>,
    #[serde(default)]
    pub tint_color: Option<String>,
}

/// Optional inline run inside a declarative `text` node.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWidgetTextRun {
    /// Text characters.
    pub text: String,
    /// Tailwind classes for this span.
    #[serde(default)]
    pub class: Option<String>,
    /// Inline style overrides for this span.
    #[serde(default)]
    pub style: Option<RawWidgetStyle>,
}

/// Declarative node produced by `ts/` JSX or builder functions.
#[derive(Debug, Clone)]
pub enum RawWidgetNode {
    /// Layout container (`box`, `row`, `col`, `stack`).
    Box {
        axis: Option<WidgetAxis>,
        class: Option<String>,
        style: Option<RawWidgetStyle>,
        widget: Option<RawWidgetHints>,
        children: Vec<RawWidgetNode>,
    },
    /// Text paragraph (`text`).
    Text {
        text: Option<String>,
        runs: Vec<RawWidgetTextRun>,
        class: Option<String>,
        style: Option<RawWidgetStyle>,
        number_of_lines: Option<u32>,
        widget: Option<RawWidgetHints>,
    },
    /// OS-driven live date (`date`).
    Date {
        date_ms: i64,
        mode: WidgetDateMode,
        class: Option<String>,
        style: Option<RawWidgetStyle>,
        widget: Option<RawWidgetHints>,
    },
    /// Image or SF Symbol (`image`).
    Image {
        source: String,
        resize_mode: Option<WidgetResizeMode>,
        class: Option<String>,
        style: Option<RawWidgetStyle>,
        widget: Option<RawWidgetHints>,
    },
    /// Linear determinate progress bar (`progress`).
    Progress {
        value: f32,
        max: Option<f32>,
        class: Option<String>,
        style: Option<Box<RawWidgetStyle>>,
        track_class: Option<String>,
        track_style: Option<Box<RawWidgetStyle>>,
        indicator_class: Option<String>,
        indicator_style: Option<Box<RawWidgetStyle>>,
    },
    /// Sub-region deep link (`link`).
    Link {
        href: String,
        accessibility_label: Option<String>,
        class: Option<String>,
        style: Option<RawWidgetStyle>,
        children: Vec<RawWidgetNode>,
    },
    /// Interactive AppIntent action button (`action`).
    Action {
        action: String,
        payload: Option<JsonValue>,
        patch: Option<JsonValue>,
        accessibility_label: Option<String>,
        class: Option<String>,
        style: Option<RawWidgetStyle>,
        children: Vec<RawWidgetNode>,
    },
}

impl<'de> Deserialize<'de> for RawWidgetNode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut raw = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let kind = raw
            .remove("kind")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::missing_field("kind"))?;
        let obj = serde_json::Value::Object(raw);
        let err = |e: serde_json::Error| serde::de::Error::custom(e.to_string());
        match kind.as_str() {
            "box" | "row" | "col" | "stack" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct BoxIn {
                    #[serde(default)]
                    axis: Option<WidgetAxis>,
                    #[serde(default)]
                    class: Option<String>,
                    #[serde(default)]
                    style: Option<RawWidgetStyle>,
                    #[serde(default)]
                    widget: Option<RawWidgetHints>,
                    #[serde(default)]
                    children: Vec<RawWidgetNode>,
                }
                let v: BoxIn = serde_json::from_value(obj).map_err(err)?;
                let axis = v.axis.or(match kind.as_str() {
                    "row" => Some(WidgetAxis::Row),
                    "col" => Some(WidgetAxis::Column),
                    "stack" => Some(WidgetAxis::Stack),
                    _ => None,
                });
                Ok(Self::Box {
                    axis,
                    class: v.class,
                    style: v.style,
                    widget: v.widget,
                    children: v.children,
                })
            }
            "text" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct TextIn {
                    #[serde(default)]
                    text: Option<String>,
                    #[serde(default)]
                    runs: Vec<RawWidgetTextRun>,
                    #[serde(default)]
                    class: Option<String>,
                    #[serde(default)]
                    style: Option<RawWidgetStyle>,
                    #[serde(default)]
                    number_of_lines: Option<u32>,
                    #[serde(default)]
                    widget: Option<RawWidgetHints>,
                }
                let v: TextIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Text {
                    text: v.text,
                    runs: v.runs,
                    class: v.class,
                    style: v.style,
                    number_of_lines: v.number_of_lines,
                    widget: v.widget,
                })
            }
            "date" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct DateIn {
                    date_ms: i64,
                    #[serde(default)]
                    mode: WidgetDateMode,
                    #[serde(default)]
                    class: Option<String>,
                    #[serde(default)]
                    style: Option<RawWidgetStyle>,
                    #[serde(default)]
                    widget: Option<RawWidgetHints>,
                }
                let v: DateIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Date {
                    date_ms: v.date_ms,
                    mode: v.mode,
                    class: v.class,
                    style: v.style,
                    widget: v.widget,
                })
            }
            "image" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ImageIn {
                    source: String,
                    #[serde(default)]
                    resize_mode: Option<WidgetResizeMode>,
                    #[serde(default)]
                    class: Option<String>,
                    #[serde(default)]
                    style: Option<RawWidgetStyle>,
                    #[serde(default)]
                    widget: Option<RawWidgetHints>,
                }
                let v: ImageIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Image {
                    source: v.source,
                    resize_mode: v.resize_mode,
                    class: v.class,
                    style: v.style,
                    widget: v.widget,
                })
            }
            "progress" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ProgressIn {
                    value: f32,
                    #[serde(default)]
                    max: Option<f32>,
                    #[serde(default)]
                    class: Option<String>,
                    #[serde(default)]
                    style: Option<Box<RawWidgetStyle>>,
                    #[serde(default)]
                    track_class: Option<String>,
                    #[serde(default)]
                    track_style: Option<Box<RawWidgetStyle>>,
                    #[serde(default)]
                    indicator_class: Option<String>,
                    #[serde(default)]
                    indicator_style: Option<Box<RawWidgetStyle>>,
                }
                let v: ProgressIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Progress {
                    value: v.value,
                    max: v.max,
                    class: v.class,
                    style: v.style,
                    track_class: v.track_class,
                    track_style: v.track_style,
                    indicator_class: v.indicator_class,
                    indicator_style: v.indicator_style,
                })
            }
            "link" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct LinkIn {
                    href: String,
                    #[serde(default)]
                    accessibility_label: Option<String>,
                    #[serde(default)]
                    class: Option<String>,
                    #[serde(default)]
                    style: Option<RawWidgetStyle>,
                    #[serde(default)]
                    children: Vec<RawWidgetNode>,
                }
                let v: LinkIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Link {
                    href: v.href,
                    accessibility_label: v.accessibility_label,
                    class: v.class,
                    style: v.style,
                    children: v.children,
                })
            }
            "action" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ActionIn {
                    action: String,
                    #[serde(default)]
                    payload: Option<JsonValue>,
                    #[serde(default)]
                    patch: Option<JsonValue>,
                    #[serde(default)]
                    accessibility_label: Option<String>,
                    #[serde(default)]
                    class: Option<String>,
                    #[serde(default)]
                    style: Option<RawWidgetStyle>,
                    #[serde(default)]
                    children: Vec<RawWidgetNode>,
                }
                let v: ActionIn = serde_json::from_value(obj).map_err(err)?;
                Ok(Self::Action {
                    action: v.action,
                    payload: v.payload,
                    patch: v.patch,
                    accessibility_label: v.accessibility_label,
                    class: v.class,
                    style: v.style,
                    children: v.children,
                })
            }
            other => Err(serde::de::Error::unknown_variant(
                other,
                &[
                    "box", "row", "col", "stack", "text", "date", "image", "progress", "link",
                    "action",
                ],
            )),
        }
    }
}

/// Default shadcn/ui neutral Light & Dark tokens matching `nsh-base`'s
/// `default-theme.json`.
pub fn default_tokens() -> Tokens {
    let mut tokens = Tokens::default();
    let pairs = [
        ("background", "oklch(1 0 0)", "oklch(0.145 0 0)"),
        ("foreground", "oklch(0.145 0 0)", "oklch(0.985 0 0)"),
        ("card", "oklch(1 0 0)", "oklch(0.205 0 0)"),
        ("card-foreground", "oklch(0.145 0 0)", "oklch(0.985 0 0)"),
        ("popover", "oklch(1 0 0)", "oklch(0.205 0 0)"),
        ("popover-foreground", "oklch(0.145 0 0)", "oklch(0.985 0 0)"),
        ("primary", "oklch(0.205 0 0)", "oklch(0.922 0 0)"),
        ("primary-foreground", "oklch(0.985 0 0)", "oklch(0.205 0 0)"),
        ("secondary", "oklch(0.97 0 0)", "oklch(0.269 0 0)"),
        (
            "secondary-foreground",
            "oklch(0.205 0 0)",
            "oklch(0.985 0 0)",
        ),
        ("muted", "oklch(0.97 0 0)", "oklch(0.269 0 0)"),
        ("muted-foreground", "oklch(0.556 0 0)", "oklch(0.708 0 0)"),
        ("accent", "oklch(0.97 0 0)", "oklch(0.269 0 0)"),
        ("accent-foreground", "oklch(0.205 0 0)", "oklch(0.985 0 0)"),
        (
            "destructive",
            "oklch(0.577 0.245 27.325)",
            "oklch(0.704 0.191 22.216)",
        ),
        (
            "destructive-foreground",
            "oklch(0.985 0 0)",
            "oklch(0.985 0 0)",
        ),
        ("border", "oklch(0.922 0 0)", "oklch(1 0 0 / 10%)"),
        ("input", "oklch(0.922 0 0)", "oklch(1 0 0 / 15%)"),
        ("ring", "oklch(0.708 0 0)", "oklch(0.556 0 0)"),
        ("chart-1", "oklch(0.87 0 0)", "oklch(0.87 0 0)"),
        ("chart-2", "oklch(0.556 0 0)", "oklch(0.556 0 0)"),
        ("chart-3", "oklch(0.439 0 0)", "oklch(0.439 0 0)"),
        ("chart-4", "oklch(0.371 0 0)", "oklch(0.371 0 0)"),
        ("chart-5", "oklch(0.269 0 0)", "oklch(0.269 0 0)"),
    ];
    for (name, light_css, dark_css) in pairs {
        if let (Some(l), Some(d)) = (Color::parse(light_css), Color::parse(dark_css)) {
            tokens.set_color(name, AdaptiveColor::new(l, d));
        }
    }
    tokens.set_number("radius", 10.0);
    tokens.set_font(FONT_SANS, "-apple-system, BlinkMacSystemFont, sans-serif");
    tokens.set_font(FONT_MONO, "ui-monospace, SFMono-Regular, monospace");
    tokens
}

/// Parses a shadcn/NSH theme JSON (`{ "cssVars": { "theme": ..., "light": ..., "dark": ... } }`)
/// over [`default_tokens`].
pub fn tokens_from_theme_json(json: &str) -> Result<Tokens, String> {
    let val: JsonValue =
        serde_json::from_str(json).map_err(|e| format!("invalid theme JSON: {e}"))?;
    let mut tokens = default_tokens();
    let Some(css_vars) = val.get("cssVars").and_then(JsonValue::as_object) else {
        return Ok(tokens);
    };
    let light_map = css_vars.get("light").and_then(JsonValue::as_object);
    let dark_map = css_vars.get("dark").and_then(JsonValue::as_object);
    if let Some(light) = light_map {
        for (key, lval) in light {
            let Some(lstr) = lval.as_str() else { continue };
            let Some(lc) = Color::parse(lstr) else {
                continue;
            };
            let dc = dark_map
                .and_then(|m| m.get(key))
                .and_then(JsonValue::as_str)
                .and_then(Color::parse)
                .unwrap_or(lc);
            tokens.set_color(key, AdaptiveColor::new(lc, dc));
        }
    }
    if let Some(dark) = dark_map {
        for (key, dval) in dark {
            if light_map.is_some_and(|m| m.contains_key(key)) {
                continue;
            }
            if let Some(dc) = dval.as_str().and_then(Color::parse) {
                let base = tokens.color(key).map(|c| c.light).unwrap_or(dc.into());
                tokens.set_color(key, AdaptiveColor::new(base, dc));
            }
        }
    }
    if let Some(theme_obj) = css_vars.get("theme").and_then(JsonValue::as_object)
        && let Some(radius_str) = theme_obj.get("radius").and_then(JsonValue::as_str)
        && let Some(px) = parse_css_length(radius_str)
    {
        tokens.set_number("radius", px);
    }
    Ok(tokens)
}

fn parse_css_length(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(rem) = s.strip_suffix("rem") {
        rem.trim().parse::<f32>().ok().map(|v| v * 16.0)
    } else if let Some(px) = s.strip_suffix("px") {
        px.trim().parse::<f32>().ok()
    } else {
        s.parse::<f32>().ok()
    }
}

/// Dual-appearance Tailwind → [`WidgetSnapshot`] compiler.
#[derive(Debug, Clone)]
pub struct WidgetCompiler {
    theme: Theme,
}

impl Default for WidgetCompiler {
    fn default() -> Self {
        Self::with_tokens(&default_tokens())
    }
}

impl WidgetCompiler {
    /// Creates a compiler with the supplied design [`Tokens`].
    pub fn with_tokens(tokens: &Tokens) -> Self {
        let mut theme = Theme::default();
        theme.set_tokens(tokens);
        Self { theme }
    }

    /// Compiles a [`RawWidgetSnapshot`] into a validated dual-appearance [`WidgetSnapshot`].
    pub fn compile_snapshot(
        &self,
        raw: &RawWidgetSnapshot,
        now_ms: i64,
    ) -> Result<WidgetSnapshot, String> {
        if raw.widget_id.trim().is_empty() {
            return Err("widgetId must not be empty".into());
        }
        if raw.entries.is_empty() {
            return Err("timeline must contain at least one entry".into());
        }
        let (reload_after_ms, reload_never) = parse_reload_policy(raw.reload_policy.as_ref())?;
        let mut entries = Vec::with_capacity(raw.entries.len());
        for entry in &raw.entries {
            let mut families = BTreeMap::new();
            for (&family, raw_tree) in &entry.families {
                families.insert(family, self.compile_tree(raw_tree, family));
            }
            entries.push(WidgetTimelineIrEntry {
                date_ms: entry.date_ms,
                relevance: entry.relevance,
                families,
            });
        }
        let snapshot = WidgetSnapshot {
            version: WIDGET_IR_VERSION,
            widget_id: raw.widget_id.clone(),
            generated_at_ms: raw.generated_at_ms.unwrap_or(now_ms),
            reload_after_ms,
            reload_never,
            entries,
        };
        snapshot.validate().map_err(|e| e.to_string())?;
        Ok(snapshot)
    }

    /// Compiles a single declarative [`RawWidgetTree`] for `family` into a
    /// dual-appearance [`WidgetFamilyTree`].
    pub fn compile_tree(&self, raw: &RawWidgetTree, family: WidgetFamily) -> WidgetFamilyTree {
        let size = family.macos_size();
        let inherited = InheritedText::default();
        let lift_bg = match &raw.root {
            RawWidgetNode::Box {
                widget: Some(h), ..
            } => h.container_background.unwrap_or(true),
            _ => true,
        };
        let mut root = self.compile_node(&raw.root, size, inherited);
        let mut container_background = None;
        if lift_bg && let WidgetNode::Box { style, .. } = &mut root {
            container_background = style.background.take();
        }
        WidgetFamilyTree {
            url: raw.url.clone(),
            container_background,
            root,
        }
    }

    fn compile_node(
        &self,
        node: &RawWidgetNode,
        window: Size<Points>,
        inherited: InheritedText,
    ) -> WidgetNode {
        match node {
            RawWidgetNode::Box {
                axis,
                class,
                style,
                widget,
                children,
            } => {
                let (light, dark, props) =
                    self.resolve_dual(class.as_deref(), style.as_ref(), window);
                let mut layout = extract_layout(&light, axis.unwrap_or(WidgetAxis::Column));
                if axis.is_none() && class_or_style_has_row(class.as_deref(), style.as_ref()) {
                    layout.axis = WidgetAxis::Row;
                }
                let child_inherited = inherited.inherit(&light, &dark, &props);
                let box_style =
                    extract_box_style(&light, &dark, widget.as_ref(), window_size_guess(&layout));
                let children = children
                    .iter()
                    .map(|c| self.compile_node(c, window, child_inherited))
                    .collect();
                WidgetNode::Box {
                    layout,
                    style: box_style,
                    children,
                }
            }
            RawWidgetNode::Text {
                text,
                runs,
                class,
                style,
                number_of_lines,
                widget,
            } => {
                let (light, dark, props) =
                    self.resolve_dual(class.as_deref(), style.as_ref(), window);
                let base_inherited = inherited.inherit(&light, &dark, &props);
                let align = map_text_align(light.text.text_align.unwrap_or(TextAlign::Auto));
                let line_limit = number_of_lines.or(props.number_of_lines).filter(|&n| n > 0);
                let layout = extract_layout(&light, WidgetAxis::Column);

                let resolved_runs = if !runs.is_empty() {
                    runs.iter()
                        .map(|run| {
                            let (rl, rd, rprops) =
                                self.resolve_dual(run.class.as_deref(), run.style.as_ref(), window);
                            let run_style = base_inherited.inherit(&rl, &rd, &rprops);
                            WidgetTextRun {
                                text: run.text.clone(),
                                style: run_style.to_widget_text_style(),
                            }
                        })
                        .collect()
                } else {
                    vec![WidgetTextRun {
                        text: text.clone().unwrap_or_default(),
                        style: base_inherited.to_widget_text_style(),
                    }]
                };

                let accentable = widget
                    .as_ref()
                    .and_then(|h| h.accentable)
                    .unwrap_or_else(|| {
                        resolved_runs
                            .iter()
                            .any(|r| r.style.color.is_some_and(|c| c.is_chromatic()))
                    });

                WidgetNode::Text {
                    runs: resolved_runs,
                    align,
                    line_limit,
                    accentable,
                    layout,
                }
            }
            RawWidgetNode::Date {
                date_ms,
                mode,
                class,
                style,
                widget,
            } => {
                let (light, dark, props) =
                    self.resolve_dual(class.as_deref(), style.as_ref(), window);
                let text_style = inherited
                    .inherit(&light, &dark, &props)
                    .to_widget_text_style();
                let accentable = widget
                    .as_ref()
                    .and_then(|h| h.accentable)
                    .unwrap_or_else(|| text_style.color.is_some_and(|c| c.is_chromatic()));
                let layout = extract_layout(&light, WidgetAxis::Column);
                WidgetNode::Date {
                    timestamp_ms: *date_ms,
                    mode: *mode,
                    style: text_style,
                    accentable,
                    layout,
                }
            }
            RawWidgetNode::Image {
                source,
                resize_mode,
                class,
                style,
                widget,
            } => {
                let (light, dark, _) = self.resolve_dual(class.as_deref(), style.as_ref(), window);
                let layout = extract_layout(&light, WidgetAxis::Column);
                let box_style =
                    extract_box_style(&light, &dark, widget.as_ref(), window_size_guess(&layout));
                let tint =
                    pair_option_color(light.view.tint_color, dark.view.tint_color).or_else(|| {
                        let c = style.as_ref().and_then(|s| s.tint_color.as_deref());
                        c.and_then(Color::parse)
                            .map(|col| WidgetColor::from_packed(col, col))
                    });
                let accented_mode = widget
                    .as_ref()
                    .and_then(|h| h.accented_rendering_mode)
                    .unwrap_or(WidgetAccentedImageMode::Desaturated);
                WidgetNode::Image {
                    source: map_image_source(source),
                    resize_mode: resize_mode
                        .unwrap_or_else(|| map_resize_mode(ResizeMode::Contain)),
                    tint,
                    accented_mode,
                    style: box_style,
                    layout,
                }
            }
            RawWidgetNode::Progress {
                value,
                max,
                class,
                style,
                track_class,
                track_style,
                indicator_class,
                indicator_style,
            } => {
                let (root_l, root_d, _) =
                    self.resolve_dual(class.as_deref(), style.as_deref(), window);
                let (track_l, track_d, _) =
                    self.resolve_dual(track_class.as_deref(), track_style.as_deref(), window);
                let (ind_l, ind_d, _) = self.resolve_dual(
                    indicator_class.as_deref(),
                    indicator_style.as_deref(),
                    window,
                );
                let mut layout = extract_layout(&root_l, WidgetAxis::Row);
                let track_layout = extract_layout(&track_l, WidgetAxis::Row);
                if layout.height.is_none() {
                    layout.height = track_layout.height;
                }
                let root_bg = extract_solid_bg(&root_l.view, &root_d.view);
                let track_bg = extract_solid_bg(&track_l.view, &track_d.view);
                let ind_bg = extract_solid_bg(&ind_l.view, &ind_d.view);
                let corner_radius = first_corner_radius(&track_l.view)
                    .or_else(|| first_corner_radius(&root_l.view));
                let denom = max.unwrap_or(100.0);
                let fraction = if denom > 0.0 {
                    (*value / denom).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                WidgetNode::Progress {
                    value: fraction,
                    track_color: track_bg.or(root_bg),
                    indicator_color: ind_bg,
                    corner_radius,
                    layout,
                }
            }
            RawWidgetNode::Link {
                href,
                accessibility_label,
                class,
                style,
                children,
            } => {
                let (light, dark, props) =
                    self.resolve_dual(class.as_deref(), style.as_ref(), window);
                let mut layout = extract_layout(&light, WidgetAxis::Row);
                if !class_or_style_has_row(class.as_deref(), style.as_ref()) {
                    layout.axis = WidgetAxis::Row;
                }
                let child_inherited = inherited.inherit(&light, &dark, &props);
                let box_style = extract_box_style(&light, &dark, None, window_size_guess(&layout));
                let compiled_children: Vec<WidgetNode> = children
                    .iter()
                    .map(|c| self.compile_node(c, window, child_inherited))
                    .collect();
                let child = WidgetNode::Box {
                    layout,
                    style: box_style,
                    children: compiled_children,
                };
                WidgetNode::Link {
                    href: href.clone(),
                    accessibility_label: accessibility_label.clone(),
                    child: Box::new(child),
                }
            }
            RawWidgetNode::Action {
                action,
                payload,
                patch,
                accessibility_label,
                class,
                style,
                children,
            } => {
                let (light, dark, props) =
                    self.resolve_dual(class.as_deref(), style.as_ref(), window);
                let layout = extract_layout(&light, WidgetAxis::Row);
                let child_inherited = inherited.inherit(&light, &dark, &props);
                let box_style = extract_box_style(&light, &dark, None, window_size_guess(&layout));
                let compiled_children: Vec<WidgetNode> = children
                    .iter()
                    .map(|c| self.compile_node(c, window, child_inherited))
                    .collect();
                let child = WidgetNode::Box {
                    layout,
                    style: box_style,
                    children: compiled_children,
                };
                WidgetNode::Action {
                    action: action.clone(),
                    payload_json: payload
                        .as_ref()
                        .filter(|v| !v.is_null())
                        .and_then(|v| serde_json::to_string(v).ok()),
                    patch_json: patch
                        .as_ref()
                        .filter(|v| !v.is_null())
                        .and_then(|v| serde_json::to_string(v).ok()),
                    accessibility_label: accessibility_label.clone(),
                    child: Box::new(child),
                }
            }
        }
    }

    fn resolve_dual(
        &self,
        class: Option<&str>,
        inline: Option<&RawWidgetStyle>,
        window: Size<Points>,
    ) -> (Style, Style, ClassProps) {
        let mut light = Style::default();
        let mut dark = Style::default();
        let mut light_props = ClassProps::default();
        let mut dark_props = ClassProps::default();
        let group = &|_: &str| 0u16;

        if let Some(classes) = class.filter(|s| !s.trim().is_empty()) {
            let list = parse(&self.theme, classes);
            let light_cx = TwContext {
                env: ClassEnvironment {
                    dark: false,
                    platform: Platform::Ios,
                    reduce_motion: false,
                    pointer: Pointer::Fine,
                    terminal: false,
                },
                window,
                rtl: false,
                states: 0,
                position: 0,
                group,
            };
            let dark_cx = TwContext {
                env: ClassEnvironment {
                    dark: true,
                    ..light_cx.env
                },
                ..light_cx
            };
            list.apply(&light_cx, &self.theme, &mut light, &mut light_props, |s| {
                if let Some(inline) = inline {
                    apply_inline_style(s, inline);
                }
            });
            list.apply(&dark_cx, &self.theme, &mut dark, &mut dark_props, |s| {
                if let Some(inline) = inline {
                    apply_inline_style(s, inline);
                }
            });
        } else if let Some(inline) = inline {
            apply_inline_style(&mut light, inline);
            apply_inline_style(&mut dark, inline);
        }
        (light, dark, light_props)
    }
}

#[derive(Debug, Clone, Copy)]
struct InheritedText {
    font_size: Points,
    font_weight: u16,
    italic: bool,
    monospaced: bool,
    light_color: Option<Color>,
    dark_color: Option<Color>,
    underline: bool,
    strikethrough: bool,
    letter_spacing: Option<Points>,
}

impl Default for InheritedText {
    fn default() -> Self {
        Self {
            font_size: Points(14.0),
            font_weight: 400,
            italic: false,
            monospaced: false,
            light_color: None,
            dark_color: None,
            underline: false,
            strikethrough: false,
            letter_spacing: None,
        }
    }
}

impl InheritedText {
    fn inherit(self, light: &Style, dark: &Style, _props: &ClassProps) -> Self {
        let lt = &light.text;
        let dt = &dark.text;
        let monospaced = lt
            .font_family
            .map(|family| {
                let name = family.name();
                nsh_core::font_families(&name).any(|f| matches!(f, FamilyName::Monospace))
            })
            .unwrap_or(self.monospaced);
        let deco = lt.decoration;
        Self {
            font_size: lt.font_size.unwrap_or(self.font_size),
            font_weight: lt.font_weight.unwrap_or(self.font_weight),
            italic: lt.italic.unwrap_or(self.italic),
            monospaced,
            light_color: lt.color.or(self.light_color),
            dark_color: dt.color.or(lt.color).or(self.dark_color),
            underline: deco.map_or(self.underline, |d| d.underline),
            strikethrough: deco.map_or(self.strikethrough, |d| d.line_through),
            letter_spacing: lt.letter_spacing.or(self.letter_spacing),
        }
    }

    fn to_widget_text_style(self) -> WidgetTextStyle {
        WidgetTextStyle {
            font_size: self.font_size,
            font_weight: self.font_weight,
            italic: self.italic,
            monospaced: self.monospaced,
            color: pair_option_color(self.light_color, self.dark_color),
            underline: self.underline,
            strikethrough: self.strikethrough,
            letter_spacing: self.letter_spacing,
        }
    }
}

fn class_or_style_has_row(class: Option<&str>, inline: Option<&RawWidgetStyle>) -> bool {
    if let Some(dir) = inline.and_then(|s| s.flex_direction.as_deref()) {
        return dir == "row" || dir == "row-reverse";
    }
    class.is_some_and(|c| c.split_ascii_whitespace().any(|t| t == "flex-row"))
}

fn apply_inline_style(style: &mut Style, inline: &RawWidgetStyle) {
    if let Some(dir) = inline.flex_direction.as_deref() {
        style.layout.flex_direction = match dir {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::Column,
        };
    }
    if let Some(jc) = inline.justify_content.as_deref() {
        style.layout.justify_content = match jc {
            "center" => JustifyContent::CENTER,
            "flex-end" | "end" => JustifyContent::FLEX_END,
            "space-between" => JustifyContent::SPACE_BETWEEN,
            "space-around" => JustifyContent::SPACE_AROUND,
            "space-evenly" => JustifyContent::SPACE_EVENLY,
            _ => JustifyContent::FLEX_START,
        };
    }
    if let Some(ai) = inline.align_items.as_deref() {
        style.layout.align_items = match ai {
            "center" => AlignItems::CENTER,
            "flex-end" | "end" => AlignItems::FLEX_END,
            "stretch" => AlignItems::STRETCH,
            _ => AlignItems::FLEX_START,
        };
    }
    if let Some(gap) = inline.gap {
        style.layout.gap = taffy::Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap),
        };
    }
    if let Some(p) = inline.padding {
        style.layout.padding = taffy::Rect::length(p);
    }
    if let Some(px) = inline.padding_horizontal {
        let lp = LengthPercentage::length(px);
        style.layout.padding.left = lp;
        style.layout.padding.right = lp;
    }
    if let Some(py) = inline.padding_vertical {
        let lp = LengthPercentage::length(py);
        style.layout.padding.top = lp;
        style.layout.padding.bottom = lp;
    }
    if let Some(v) = inline.padding_top {
        style.layout.padding.top = LengthPercentage::length(v);
    }
    if let Some(v) = inline.padding_right {
        style.layout.padding.right = LengthPercentage::length(v);
    }
    if let Some(v) = inline.padding_bottom {
        style.layout.padding.bottom = LengthPercentage::length(v);
    }
    if let Some(v) = inline.padding_left {
        style.layout.padding.left = LengthPercentage::length(v);
    }
    if let Some(w) = inline.width {
        style.layout.size.width = taffy::Dimension::length(w);
    }
    if let Some(h) = inline.height {
        style.layout.size.height = taffy::Dimension::length(h);
    }
    if let Some(w) = inline.min_width {
        style.layout.min_size.width = LengthPercentageAuto::length(w);
    }
    if let Some(h) = inline.min_height {
        style.layout.min_size.height = LengthPercentageAuto::length(h);
    }
    if let Some(w) = inline.max_width {
        style.layout.max_size.width = LengthPercentageAuto::length(w);
    }
    if let Some(h) = inline.max_height {
        style.layout.max_size.height = LengthPercentageAuto::length(h);
    }
    if let Some(fg) = inline.flex_grow {
        style.layout.flex_grow = fg;
    }
    if let Some(bg) = inline.background_color.as_deref().and_then(Color::parse) {
        style.view.background_color = bg;
    }
    if let Some(bc) = inline.border_color.as_deref().and_then(Color::parse) {
        style.view.borders.colors = [bc; 4];
    }
    if let Some(bw) = inline.border_width {
        style.view.borders.widths = nsh_core::Edges::all(Points(bw));
    }
    if let Some(br) = inline.border_radius {
        style.view.border_radius = Points(br);
    }
    if let Some(op) = inline.opacity {
        style.view.opacity = op;
    }
    if inline.overflow.as_deref() == Some("hidden") {
        style.view.overflow_hidden = true;
    }
    if let Some(col) = inline.color.as_deref().and_then(Color::parse) {
        style.text.color = Some(col);
    }
    if let Some(fs) = inline.font_size {
        style.text.font_size = Some(Points(fs));
    }
    if let Some(fw) = inline.font_weight {
        style.text.font_weight = Some(fw);
    }
    if let Some(fs) = inline.font_style.as_deref() {
        style.text.italic = Some(fs == "italic");
    }
    if let Some(ff) = inline.font_family.as_deref() {
        style.text.font_family = Some(FontFamily::new(ff));
    }
    if let Some(ls) = inline.letter_spacing {
        style.text.letter_spacing = Some(Points(ls));
    }
    if let Some(ta) = inline.text_align.as_deref() {
        style.text.text_align = Some(match ta {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            "left" => TextAlign::Left,
            _ => TextAlign::Auto,
        });
    }
    if let Some(tc) = inline.tint_color.as_deref().and_then(Color::parse) {
        style.view.tint_color = Some(tc);
    }
}

fn parse_reload_policy(value: Option<&JsonValue>) -> Result<(Option<i64>, bool), String> {
    let Some(value) = value else {
        return Ok((None, false));
    };
    match value {
        JsonValue::Null => Ok((None, false)),
        JsonValue::String(s) if s == "atEnd" => Ok((None, false)),
        JsonValue::String(s) if s == "never" => Ok((None, true)),
        JsonValue::Object(map) => {
            let Some(after) = map.get("after").and_then(JsonValue::as_f64) else {
                return Err(
                    "reloadPolicy object must have a finite numeric `after` (milliseconds)".into(),
                );
            };
            if !after.is_finite() || after <= 0.0 {
                return Err("reloadPolicy.after must be a positive number of milliseconds".into());
            }
            Ok((Some(after as i64), false))
        }
        _ => Err("reloadPolicy must be \"atEnd\", \"never\", or { after: number }".into()),
    }
}

fn extract_layout(style: &Style, default_axis: WidgetAxis) -> WidgetLayout {
    let ls = &style.layout;
    let axis = if ls.position == taffy::Position::Absolute || default_axis == WidgetAxis::Stack {
        WidgetAxis::Stack
    } else {
        match ls.flex_direction {
            FlexDirection::Row | FlexDirection::RowReverse => WidgetAxis::Row,
            FlexDirection::Column | FlexDirection::ColumnReverse => match default_axis {
                WidgetAxis::Row => WidgetAxis::Row,
                _ => WidgetAxis::Column,
            },
        }
    };
    let main_align = match ls.justify_content {
        c if c == JustifyContent::CENTER => WidgetMainAlign::Center,
        c if c == JustifyContent::FLEX_END || c == JustifyContent::END => WidgetMainAlign::End,
        c if c == JustifyContent::SPACE_BETWEEN => WidgetMainAlign::SpaceBetween,
        c if c == JustifyContent::SPACE_AROUND => WidgetMainAlign::SpaceAround,
        c if c == JustifyContent::SPACE_EVENLY => WidgetMainAlign::SpaceEvenly,
        _ => WidgetMainAlign::Start,
    };
    let cross_align = match ls.align_items {
        c if c == AlignItems::CENTER => WidgetCrossAlign::Center,
        c if c == AlignItems::FLEX_END || c == AlignItems::END => WidgetCrossAlign::End,
        c if c == AlignItems::STRETCH => WidgetCrossAlign::Stretch,
        _ => WidgetCrossAlign::Start,
    };
    let gap = match axis {
        WidgetAxis::Row => length_pct_points(ls.gap.width),
        WidgetAxis::Column | WidgetAxis::Stack => length_pct_points(ls.gap.height),
    };
    WidgetLayout {
        axis,
        main_align,
        cross_align,
        gap,
        padding: WidgetEdges {
            top: length_pct_points(ls.padding.top),
            right: length_pct_points(ls.padding.right),
            bottom: length_pct_points(ls.padding.bottom),
            left: length_pct_points(ls.padding.left),
        },
        width: dimension_points(ls.size.width),
        height: dimension_points(ls.size.height),
        min_width: lpa_points(ls.min_size.width),
        min_height: lpa_points(ls.min_size.height),
        max_width: lpa_points(ls.max_size.width),
        max_height: lpa_points(ls.max_size.height),
        flex_grow: ls.flex_grow,
    }
}

fn length_pct_points(value: LengthPercentage) -> Points {
    let raw = value.into_raw();
    if raw.tag() == taffy::CompactLength::LENGTH_TAG {
        Points(raw.value())
    } else {
        Points(0.0)
    }
}

fn dimension_points(value: taffy::Dimension) -> Option<Points> {
    let raw = value.into_raw();
    if raw.tag() == taffy::CompactLength::LENGTH_TAG {
        Some(Points(raw.value()))
    } else {
        None
    }
}

fn lpa_points(value: LengthPercentageAuto) -> Option<Points> {
    let raw = value.into_raw();
    if raw.tag() == taffy::CompactLength::LENGTH_TAG {
        Some(Points(raw.value()))
    } else {
        None
    }
}

fn window_size_guess(layout: &WidgetLayout) -> Size<Points> {
    Size {
        width: layout.width.unwrap_or(Points(364.0)),
        height: layout.height.unwrap_or(Points(170.0)),
    }
}

fn pair_option_color(light: Option<Color>, dark: Option<Color>) -> Option<WidgetColor> {
    match (light, dark) {
        (Some(l), Some(d)) => Some(WidgetColor::from_packed(l, d)),
        (Some(l), None) => Some(WidgetColor::from_packed(l, l)),
        (None, Some(d)) => Some(WidgetColor::from_packed(d, d)),
        (None, None) => None,
    }
}

fn extract_solid_bg(light: &ViewStyle, dark: &ViewStyle) -> Option<WidgetColor> {
    let l = light.background_color;
    let d = dark.background_color;
    if l.is_transparent() && d.is_transparent() {
        None
    } else {
        Some(WidgetColor::from_packed(l, d))
    }
}

fn extract_fill(light: &ViewStyle, dark: &ViewStyle, size: Size<Points>) -> Option<WidgetFill> {
    if let Some(effects) = light.effects.as_deref()
        && let Some(l_grad) = effects.background_images.first()
        && let Some(angle) = l_grad.kind.linear_angle(size)
    {
        let d_grad = dark
            .effects
            .as_deref()
            .and_then(|e| e.background_images.first())
            .unwrap_or(l_grad);
        let l_stops = l_grad.resolved_stops(size);
        let d_stops = d_grad.resolved_stops(size);
        let stops = l_stops
            .iter()
            .enumerate()
            .map(|(i, &(lc, pos))| {
                let dc = d_stops.get(i).map_or(lc, |&(c, _)| c);
                WidgetGradientStop {
                    color: WidgetColor::from_packed(lc, dc),
                    position: pos.0,
                }
            })
            .collect();
        return Some(WidgetFill::LinearGradient {
            angle_deg: angle.0.to_degrees(),
            stops,
        });
    }
    extract_solid_bg(light, dark).map(|color| WidgetFill::Solid { color })
}

fn first_corner_radius(vs: &ViewStyle) -> Option<Points> {
    let c = vs.corner_radius;
    c.top_left
        .or(c.top_right)
        .or(c.bottom_left)
        .or(c.bottom_right)
        .or((vs.border_radius.0 > 0.0).then_some(vs.border_radius))
}

fn extract_box_style(
    light: &Style,
    dark: &Style,
    hints: Option<&RawWidgetHints>,
    size: Size<Points>,
) -> WidgetBoxStyle {
    let l_vs = &light.view;
    let d_vs = &dark.view;
    let background = extract_fill(l_vs, d_vs, size);
    let w = &l_vs.borders.widths;
    let border_width = w.top.0.max(w.right.0).max(w.bottom.0).max(w.left.0);
    let border_color = if border_width > 0.0 {
        let pick_border = |vs: &ViewStyle| {
            let bw = &vs.borders.widths;
            let sides = [bw.top.0, bw.right.0, bw.bottom.0, bw.left.0];
            sides
                .iter()
                .enumerate()
                .find(|&(_, &width)| width > 0.0)
                .map(|(i, _)| vs.borders.colors[i])
        };
        pair_option_color(pick_border(l_vs), pick_border(d_vs))
    } else {
        None
    };
    let corner_radius = first_corner_radius(l_vs).unwrap_or(Points(0.0));
    let opacity = l_vs.opacity;
    let clip = l_vs.overflow_hidden || corner_radius.0 > 0.0;
    let accentable = hints.and_then(|h| h.accentable).unwrap_or_else(
        || matches!(&background, Some(WidgetFill::Solid { color }) if color.is_chromatic()),
    );
    WidgetBoxStyle {
        background,
        border_color,
        border_width: Points(border_width),
        corner_radius,
        opacity,
        clip,
        accentable,
    }
}

fn map_text_align(align: TextAlign) -> WidgetTextAlign {
    match align {
        TextAlign::Center => WidgetTextAlign::Center,
        TextAlign::Right => WidgetTextAlign::Trailing,
        TextAlign::Auto | TextAlign::Left | TextAlign::Justify => WidgetTextAlign::Leading,
    }
}

fn map_resize_mode(mode: ResizeMode) -> WidgetResizeMode {
    match mode {
        ResizeMode::Cover => WidgetResizeMode::Cover,
        ResizeMode::Stretch => WidgetResizeMode::Stretch,
        ResizeMode::Center => WidgetResizeMode::Center,
        ResizeMode::Contain | ResizeMode::ScaleDown => WidgetResizeMode::Contain,
    }
}

fn map_image_source(uri: &str) -> WidgetImageSource {
    if let Some(symbol) = uri.strip_prefix("symbol:") {
        WidgetImageSource::Symbol {
            name: symbol.to_owned(),
        }
    } else if let Some(asset) = uri
        .strip_prefix("asset:/")
        .or_else(|| uri.strip_prefix("asset:"))
    {
        WidgetImageSource::Asset {
            path: asset.trim_start_matches('/').to_owned(),
        }
    } else if let Some(file) = uri.strip_prefix("file://") {
        WidgetImageSource::File {
            path: file.to_owned(),
        }
    } else {
        WidgetImageSource::Asset {
            path: uri.trim_start_matches('/').to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_dual_appearance_tailwind_tokens_and_layout() {
        let compiler = WidgetCompiler::default();
        let raw: RawWidgetSnapshot = serde_json::from_value(serde_json::json!({
            "widgetId": "portfolio",
            "reloadPolicy": { "after": 900_000 },
            "entries": [{
                "dateMs": 1_795_280_000_000i64,
                "relevance": 0.9,
                "families": {
                    "medium": {
                        "url": "pulse://symbol/AAPL",
                        "root": {
                            "kind": "col",
                            "class": "flex-1 justify-between p-4 bg-card",
                            "children": [
                                {
                                    "kind": "row",
                                    "class": "items-center justify-between gap-2",
                                    "children": [
                                        {
                                            "kind": "text",
                                            "text": "AAPL",
                                            "class": "text-sm font-semibold text-foreground"
                                        },
                                        {
                                            "kind": "action",
                                            "action": "refresh",
                                            "payload": { "symbol": "AAPL" },
                                            "class": "px-2.5 py-1 rounded-md bg-secondary",
                                            "children": [{
                                                "kind": "text",
                                                "text": "Refresh",
                                                "class": "text-xs font-medium text-secondary-foreground"
                                            }]
                                        }
                                    ]
                                },
                                {
                                    "kind": "text",
                                    "text": "+1.42%",
                                    "class": "text-xs font-semibold text-emerald-500",
                                    "widget": { "accentable": true }
                                },
                                {
                                    "kind": "progress",
                                    "value": 72,
                                    "class": "h-1.5 rounded-full bg-muted",
                                    "indicatorClass": "bg-primary"
                                },
                                {
                                    "kind": "link",
                                    "href": "pulse://symbol/AAPL/chart",
                                    "children": [{
                                        "kind": "date",
                                        "dateMs": 1_795_280_000_000i64,
                                        "mode": "relative",
                                        "class": "text-xs text-muted-foreground"
                                    }]
                                }
                            ]
                        }
                    }
                }
            }]
        }))
        .unwrap();

        let snapshot = compiler.compile_snapshot(&raw, 1_795_280_000_000).unwrap();
        assert_eq!(snapshot.reload_after_ms, Some(900_000));
        let tree = &snapshot.entries[0].families[&WidgetFamily::Medium];
        assert_eq!(tree.url.as_deref(), Some("pulse://symbol/AAPL"));
        // Root bg-card lifted into containerBackground and differs between Light & Dark.
        let Some(WidgetFill::Solid { color: bg }) = tree.container_background else {
            panic!("expected lifted solid containerBackground");
        };
        assert_ne!(bg.light, bg.dark);
        assert!(bg.light[0] > 0.9 && bg.dark[0] < 0.3);

        let WidgetNode::Box {
            layout,
            style,
            children,
        } = &tree.root
        else {
            panic!("expected root box");
        };
        assert!(style.background.is_none());
        assert_eq!(layout.axis, WidgetAxis::Column);
        assert_eq!(layout.main_align, WidgetMainAlign::SpaceBetween);
        assert_eq!(layout.padding.top, Points(16.0));
        assert_eq!(children.len(), 4);

        let WidgetNode::Text {
            runs, accentable, ..
        } = &children[1]
        else {
            panic!("expected emerald text");
        };
        assert!(*accentable);
        assert!(runs[0].style.color.is_some());
        assert_eq!(runs[0].style.font_weight, 600);

        let WidgetNode::Progress {
            value,
            track_color,
            indicator_color,
            ..
        } = &children[2]
        else {
            panic!("expected progress");
        };
        assert!((*value - 0.72).abs() < 0.001);
        assert!(track_color.is_some());
        assert!(indicator_color.is_some());
    }
}
