//! Atomic file persistence inside the shared App Group (or local fallback)
//! container: `widgets/<id>/snapshot.json`, `widgets/<id>/data.json`,
//! `widgets/actions.jsonl`, and `widgets/assets/`.

use std::{
    collections::BTreeSet,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use nsh_core::JsonValue;

use crate::{
    config::WidgetActionEvent,
    ir::{WidgetImageSource, WidgetNode, WidgetSnapshot},
};

/// Shared container file store read by the SwiftUI WidgetKit extension and
/// written by the main Native Shell app.
#[derive(Debug, Clone)]
pub struct WidgetStore {
    root: PathBuf,
}

impl WidgetStore {
    /// Creates a store anchored at `<container>/widgets`.
    pub fn new(container_dir: impl AsRef<Path>) -> Self {
        Self {
            root: container_dir.as_ref().join("widgets"),
        }
    }

    /// Root `widgets/` directory inside the container.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path to a widget's snapshot JSON file (`widgets/<id>/snapshot.json`).
    pub fn snapshot_path(&self, widget_id: &str) -> PathBuf {
        self.root.join(widget_id).join("snapshot.json")
    }

    /// Path to a widget's persisted raw data JSON file (`widgets/<id>/data.json`).
    pub fn data_path(&self, widget_id: &str) -> PathBuf {
        self.root.join(widget_id).join("data.json")
    }

    /// Path to the queued offline widget actions file (`widgets/actions.jsonl`).
    pub fn actions_path(&self) -> PathBuf {
        self.root.join("actions.jsonl")
    }

    /// Path to copied widget image assets (`widgets/assets/`).
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    /// Validates `snapshot`, copies any referenced `Asset` images from
    /// `source_assets_dir` if provided, and atomically writes `snapshot.json`.
    pub fn write_snapshot(
        &self,
        snapshot: &WidgetSnapshot,
        source_assets_dir: Option<&Path>,
    ) -> Result<(), String> {
        snapshot.validate().map_err(|e| e.to_string())?;
        if let Some(src_dir) = source_assets_dir {
            let mut assets = BTreeSet::new();
            collect_snapshot_assets(snapshot, &mut assets);
            for rel in assets {
                if rel.is_empty() || rel.contains("..") || rel.starts_with('/') {
                    continue;
                }
                let src = src_dir.join(&rel);
                if src.is_file() {
                    let dst = self.assets_dir().join(&rel);
                    if let Some(parent) = dst.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("create widget asset dir: {e}"))?;
                    }
                    let tmp = dst.with_extension("tmp");
                    fs::copy(&src, &tmp).map_err(|e| format!("copy widget asset {rel}: {e}"))?;
                    fs::rename(&tmp, &dst)
                        .map_err(|e| format!("commit widget asset {rel}: {e}"))?;
                }
            }
        }
        let bytes =
            serde_json::to_vec(snapshot).map_err(|e| format!("serialize widget snapshot: {e}"))?;
        atomic_write(&self.snapshot_path(&snapshot.widget_id), &bytes)
    }

    /// Reads the persisted [`WidgetSnapshot`] for `widget_id`, if present.
    pub fn read_snapshot(&self, widget_id: &str) -> Result<Option<WidgetSnapshot>, String> {
        let path = self.snapshot_path(widget_id);
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let snapshot =
            serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
        Ok(Some(snapshot))
    }

    /// Persists raw JSON `data` for `widget_id` atomically (`data.json`).
    pub fn write_data(&self, widget_id: &str, data: &JsonValue) -> Result<(), String> {
        let bytes = serde_json::to_vec(data).map_err(|e| format!("serialize widget data: {e}"))?;
        atomic_write(&self.data_path(widget_id), &bytes)
    }

    /// Reads the last persisted raw JSON `data` for `widget_id`.
    pub fn read_data(&self, widget_id: &str) -> Result<Option<JsonValue>, String> {
        let path = self.data_path(widget_id);
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let data =
            serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
        Ok(Some(data))
    }

    /// Appends a [`WidgetActionEvent`] to `actions.jsonl` (primarily for tests).
    pub fn enqueue_action(&self, event: &WidgetActionEvent) -> Result<(), String> {
        let path = self.actions_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create widget store dir: {e}"))?;
        }
        let line = serde_json::to_string(event).map_err(|e| format!("serialize action: {e}"))?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("open {}: {e}", path.display()))?;
        writeln!(file, "{line}").map_err(|e| format!("write {}: {e}", path.display()))?;
        Ok(())
    }

    /// Atomically drains and clears all queued [`WidgetActionEvent`] entries
    /// from `actions.jsonl`.
    pub fn drain_actions(&self) -> Vec<WidgetActionEvent> {
        let path = self.actions_path();
        if !path.is_file() {
            return Vec::new();
        }
        let draining = path.with_extension(format!("draining-{}", std::process::id()));
        if fs::rename(&path, &draining).is_err() {
            return Vec::new();
        }
        let content = fs::read_to_string(&draining).unwrap_or_default();
        let _ = fs::remove_file(&draining);
        content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<WidgetActionEvent>(line).ok())
            .collect()
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create directory {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename to {}: {e}", path.display()))?;
    Ok(())
}

fn collect_snapshot_assets(snapshot: &WidgetSnapshot, out: &mut BTreeSet<String>) {
    for entry in &snapshot.entries {
        for tree in entry.families.values() {
            collect_node_assets(&tree.root, out);
        }
    }
}

fn collect_node_assets(node: &WidgetNode, out: &mut BTreeSet<String>) {
    match node {
        WidgetNode::Box { children, .. } => {
            for child in children {
                collect_node_assets(child, out);
            }
        }
        WidgetNode::Link { child, .. } | WidgetNode::Action { child, .. } => {
            collect_node_assets(child, out);
        }
        WidgetNode::Image {
            source: WidgetImageSource::Asset { path },
            ..
        } => {
            out.insert(path.clone());
        }
        WidgetNode::Text { .. }
        | WidgetNode::Date { .. }
        | WidgetNode::Image { .. }
        | WidgetNode::Progress { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{WidgetBoxStyle, WidgetLayout, WidgetTimelineIrEntry};
    use std::collections::BTreeMap;

    #[test]
    fn round_trips_snapshot_data_and_actions() {
        let dir = std::env::temp_dir().join(format!("nsh-widget-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = WidgetStore::new(&dir);

        let mut families = BTreeMap::new();
        families.insert(
            crate::config::WidgetFamily::Small,
            crate::ir::WidgetFamilyTree {
                url: None,
                container_background: None,
                root: WidgetNode::Box {
                    layout: WidgetLayout::default(),
                    style: WidgetBoxStyle::default(),
                    children: Vec::new(),
                },
            },
        );
        let snapshot = WidgetSnapshot {
            version: crate::ir::WIDGET_IR_VERSION,
            widget_id: "portfolio".into(),
            generated_at_ms: 1000,
            reload_after_ms: None,
            reload_never: false,
            entries: vec![WidgetTimelineIrEntry {
                date_ms: 1000,
                relevance: None,
                families,
            }],
        };

        store.write_snapshot(&snapshot, None).unwrap();
        assert_eq!(store.read_snapshot("portfolio").unwrap(), Some(snapshot));

        let payload = serde_json::json!({ "symbol": "AAPL" });
        store.write_data("portfolio", &payload).unwrap();
        assert_eq!(store.read_data("portfolio").unwrap(), Some(payload));

        store
            .enqueue_action(&WidgetActionEvent {
                widget_id: "portfolio".into(),
                action: "refresh".into(),
                payload: Some(serde_json::json!({ "symbol": "AAPL" })),
                timestamp: 1234,
            })
            .unwrap();
        let drained = store.drain_actions();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].action, "refresh");
        assert!(store.drain_actions().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
