//! Widget configuration types (`nsh-widget.json`), timeline reload policy,
//! and action event payloads.

use std::collections::BTreeSet;

use nsh_core::{
    JsonValue,
    element::PropEnum,
    geometry::{Points, Size},
};
use serde::{Deserialize, Serialize};

/// Supported widget size families across Apple platforms.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    PropEnum,
)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WidgetFamily {
    /// Compact square card (170 × 170 pt on macOS).
    #[default]
    Small,
    /// Wide horizontal card (364 × 170 pt on macOS).
    Medium,
    /// Tall square-ish card (364 × 382 pt on macOS).
    Large,
}

impl WidgetFamily {
    /// Canonical macOS WidgetKit logical size in points.
    pub const fn macos_size(self) -> Size<Points> {
        match self {
            Self::Small => Size {
                width: Points(170.0),
                height: Points(170.0),
            },
            Self::Medium => Size {
                width: Points(364.0),
                height: Points(170.0),
            },
            Self::Large => Size {
                width: Points(364.0),
                height: Points(382.0),
            },
        }
    }

    /// Its canonical JSON name (`"small"`, `"medium"`, `"large"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// Its SwiftUI `WidgetFamily` case name (`".systemSmall"`, etc.).
    pub const fn swift_case(self) -> &'static str {
        match self {
            Self::Small => ".systemSmall",
            Self::Medium => ".systemMedium",
            Self::Large => ".systemLarge",
        }
    }

    /// Parses a family name (`"small"`, `"medium"`, `"large"`).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }
}

/// Timeline reload policy hint passed to WidgetKit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WidgetReloadPolicy {
    /// Ask WidgetKit to reload after the last timeline entry (`atEnd`).
    #[default]
    AtEnd,
    /// Tell WidgetKit not to reload until the main app calls `Widget.reload`.
    Never,
    /// Ask WidgetKit to reload after `ms` milliseconds from generation.
    After(i64),
}

/// Interactive button action event persisted by `NshWidgetActionIntent` in
/// `actions.jsonl` and emitted to `Widget.on("action", ...)` in JS.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetActionEvent {
    /// Widget id declared in the extension / `Widget.update`.
    pub widget_id: String,
    /// Action name attached to `<WAction>`.
    pub action: String,
    /// Optional JSON payload attached to `<WAction>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
    /// Unix timestamp in milliseconds when the user clicked the action.
    pub timestamp: i64,
}

/// One widget entry declared in `nsh-widget.json` `"widgets"`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetEntryConfig {
    /// Stable widget identifier (`[a-z0-9][a-z0-9._-]{0,63}`), unique in the app.
    pub id: String,
    /// Display name shown in the OS widget gallery.
    pub name: String,
    /// Short description shown in the OS widget gallery.
    #[serde(default)]
    pub description: String,
    /// Supported widget size families (at least one).
    pub families: Vec<WidgetFamily>,
    /// Optional raw widget tree (`RawWidgetTree`) used to generate the
    /// compile-time `default-snapshots.json` bundled inside `.appex`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tree: Option<JsonValue>,
}

/// Project-level widget extension build configuration (`nsh-widget.json`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WidgetProjectConfig {
    /// Optional explicit App Group identifier (defaults to the first entry in
    /// `nsh.json` `macos.entitlements["com.apple.security.application-groups"]`,
    /// or `nsh.local.<appId>.widgets`).
    #[serde(default)]
    pub app_group: Option<String>,
    /// Optional explicit bundle identifier for `.appex` (defaults to `<appId>.widgets`).
    #[serde(default)]
    pub bundle_id: Option<String>,
    /// Optional executable & `.appex` name without extension (defaults to `"Widgets"`).
    #[serde(default)]
    pub executable: Option<String>,
    /// Optional output path relative to the app directory (defaults to
    /// `macos/build/<executable>.appex`).
    #[serde(default)]
    pub output: Option<String>,
    /// Declared widgets built into the `.appex` `WidgetBundle`.
    #[serde(default)]
    pub widgets: Vec<WidgetEntryConfig>,
}

/// Checks whether `id` is a valid widget identifier: ASCII
/// `[a-z0-9][a-z0-9._-]{0,63}`.
pub fn is_valid_widget_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

/// Validates a list of widget configurations from `nsh-widget.json`.
pub fn validate_widgets(entries: &[WidgetEntryConfig]) -> Result<(), String> {
    if entries.is_empty() {
        return Err("nsh-widget.json: `widgets` must contain at least one widget".into());
    }
    let mut seen_ids = BTreeSet::new();
    for (index, widget) in entries.iter().enumerate() {
        let prefix = format!("nsh-widget.json widgets[{index}]");
        if !is_valid_widget_id(&widget.id) {
            return Err(format!(
                "{prefix}.id: {:?} must match [a-z0-9][a-z0-9._-]{{0,63}}",
                widget.id
            ));
        }
        if !seen_ids.insert(widget.id.as_str()) {
            return Err(format!("{prefix}.id: duplicate widget id {:?}", widget.id));
        }
        if widget.name.trim().is_empty() {
            return Err(format!("{prefix}.name: must not be empty"));
        }
        if widget.families.is_empty() {
            return Err(format!(
                "{prefix}.families: must declare at least one family (\"small\", \"medium\", \"large\")"
            ));
        }
        let mut seen_families = BTreeSet::new();
        for family in &widget.families {
            if !seen_families.insert(*family) {
                return Err(format!(
                    "{prefix}.families: duplicate family {:?}",
                    family.as_str()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_widget_configs() {
        let cfg: WidgetProjectConfig = serde_json::from_value(serde_json::json!({
            "widgets": [{
                "id": "portfolio",
                "name": "Portfolio",
                "description": "Watchlist prices.",
                "families": ["small", "medium", "large"]
            }]
        }))
        .unwrap();
        validate_widgets(&cfg.widgets).unwrap();

        let bad_id: Vec<WidgetEntryConfig> = serde_json::from_value(serde_json::json!([
            { "id": "Bad_ID", "name": "W", "families": ["small"] }
        ]))
        .unwrap();
        assert!(validate_widgets(&bad_id).unwrap_err().contains(".id:"));
    }
}
