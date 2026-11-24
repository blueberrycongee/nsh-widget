//! Zero-Xcode macOS WidgetKit `.appex` compiler and bundle assembler.
//!
//! Reads the app's `nsh.json` and `nsh-widget.json`, compiles the embedded
//! `NshWidgetRuntime` Swift sources plus a generated `WidgetsMain.swift` entry
//! point with `xcrun --sdk macosx swiftc`, and outputs a signed-ready
//! `<Output>.appex` bundle that `nsh package --target macos` embeds via
//! `macos.extensions`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

use crate::{
    compiler::{
        RawWidgetNode, RawWidgetSnapshot, RawWidgetTimelineEntry, RawWidgetTree, WidgetCompiler,
        tokens_from_theme_json,
    },
    config::{WidgetEntryConfig, WidgetProjectConfig, validate_widgets},
    host::now_ms,
};

const WIDGET_MIN_MACOS: &str = "14.0";

const SWIFT_RUNTIME_FILES: &[(&str, &str)] = &[
    (
        "WidgetIR.swift",
        include_str!("../swift/runtime/WidgetIR.swift"),
    ),
    (
        "WidgetRenderer.swift",
        include_str!("../swift/runtime/WidgetRenderer.swift"),
    ),
    (
        "WidgetStore.swift",
        include_str!("../swift/runtime/WidgetStore.swift"),
    ),
    (
        "WidgetIntent.swift",
        include_str!("../swift/runtime/WidgetIntent.swift"),
    ),
    (
        "WidgetProvider.swift",
        include_str!("../swift/runtime/WidgetProvider.swift"),
    ),
];

/// Target CPU architecture(s) for the `.appex` Mach-O binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetArch {
    Arm64,
    X86_64,
    Universal,
}

impl TargetArch {
    /// Host machine architecture.
    pub fn host() -> Self {
        if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else {
            Self::X86_64
        }
    }

    /// Parses `"arm64"`, `"aarch64"`, `"x86_64"`, or `"universal"`.
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "arm64" | "aarch64" => Ok(Self::Arm64),
            "x86_64" => Ok(Self::X86_64),
            "universal" => Ok(Self::Universal),
            other => bail!("unsupported arch {other:?}; expected arm64, x86_64, or universal"),
        }
    }

    fn slices(self) -> &'static [&'static str] {
        match self {
            Self::Arm64 => &["arm64"],
            Self::X86_64 => &["x86_64"],
            Self::Universal => &["arm64", "x86_64"],
        }
    }
}

/// Options for [`build_macos_appex`].
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// App directory containing `nsh.json`.
    pub app_dir: PathBuf,
    /// Optional explicit path to `nsh-widget.json` (defaults to `<app_dir>/nsh-widget.json`).
    pub config_path: Option<PathBuf>,
    /// Optional explicit output `.appex` directory.
    pub output_path: Option<PathBuf>,
    /// Target architecture(s).
    pub arch: TargetArch,
    /// Optional override for the shared App Group identifier.
    pub app_group: Option<String>,
    /// Optional override for the `.appex` `CFBundleIdentifier`.
    pub bundle_id: Option<String>,
    /// Optional override for the `.appex` executable name.
    pub executable: Option<String>,
}

/// Output summary of a built `.appex` bundle.
#[derive(Debug, Clone)]
pub struct BuildOutput {
    pub appex_path: PathBuf,
    pub bundle_id: String,
    pub app_group: String,
    pub widget_count: usize,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppNshConfig {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    build: Option<serde_json::Value>,
    theme: Option<String>,
    #[serde(default)]
    macos: AppMacosSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppMacosSection {
    #[serde(default)]
    entitlements: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    extensions: Vec<AppExtensionEntry>,
    minimum_system_version: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppExtensionEntry {
    path: Option<String>,
    #[serde(default)]
    entitlements: serde_json::Map<String, serde_json::Value>,
}

/// Builds a macOS WidgetKit `.appex` bundle for the app at `opts.app_dir`.
pub fn build_macos_appex(opts: BuildOptions) -> anyhow::Result<BuildOutput> {
    if !cfg!(target_os = "macos") {
        bail!("building a macOS .appex requires macOS with Xcode / Command Line Tools");
    }
    let app_dir = fs::canonicalize(&opts.app_dir)
        .with_context(|| format!("app directory not found: {}", opts.app_dir.display()))?;
    let nsh_json_path = app_dir.join("nsh.json");
    let nsh_raw = fs::read_to_string(&nsh_json_path)
        .with_context(|| format!("read {}", nsh_json_path.display()))?;
    let app_cfg: AppNshConfig = serde_json::from_str(&nsh_raw)
        .with_context(|| format!("parse {}", nsh_json_path.display()))?;

    let widget_cfg_path = opts
        .config_path
        .unwrap_or_else(|| app_dir.join("nsh-widget.json"));
    let widget_raw = fs::read_to_string(&widget_cfg_path)
        .with_context(|| format!("read {}", widget_cfg_path.display()))?;
    let widget_cfg: WidgetProjectConfig = serde_json::from_str(&widget_raw)
        .with_context(|| format!("parse {}", widget_cfg_path.display()))?;
    validate_widgets(&widget_cfg.widgets).map_err(|e| anyhow::anyhow!("{e}"))?;

    let app_id = app_cfg
        .id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .context("nsh.json must set \"id\" (e.g. \"com.example.pulse\") so the widget extension identifier can start with `<id>.`")?;

    let app_group = opts
        .app_group
        .or_else(|| widget_cfg.app_group.clone())
        .or_else(|| first_app_group(&app_cfg.macos.entitlements))
        .or_else(|| {
            app_cfg
                .macos
                .extensions
                .first()
                .and_then(|e| first_app_group(&e.entitlements))
        })
        .unwrap_or_else(|| format!("nsh.local.{app_id}.widgets"));

    let bundle_id = opts
        .bundle_id
        .or_else(|| widget_cfg.bundle_id.clone())
        .unwrap_or_else(|| format!("{app_id}.widgets"));
    if bundle_id
        .strip_prefix(app_id)
        .and_then(|rest| rest.strip_prefix('.'))
        .is_none_or(str::is_empty)
    {
        bail!("extension bundle id {bundle_id:?} must start with `{app_id}.`");
    }

    let executable = opts
        .executable
        .or_else(|| widget_cfg.executable.clone())
        .unwrap_or_else(|| "Widgets".to_owned());

    let appex_path = opts
        .output_path
        .or_else(|| widget_cfg.output.as_ref().map(|rel| app_dir.join(rel)))
        .or_else(|| {
            app_cfg
                .macos
                .extensions
                .first()
                .and_then(|e| e.path.as_ref())
                .map(|rel| app_dir.join(rel))
        })
        .unwrap_or_else(|| {
            app_dir
                .join("macos/build")
                .join(format!("{executable}.appex"))
        });

    let build_dir = app_dir.join(".nsh/build/nsh-widget");
    let runtime_src_dir = build_dir.join("runtime-src");
    fs::create_dir_all(&runtime_src_dir)?;
    let mut swift_sources = Vec::with_capacity(SWIFT_RUNTIME_FILES.len());
    for &(name, contents) in SWIFT_RUNTIME_FILES {
        let p = runtime_src_dir.join(name);
        write_if_changed(&p, contents.as_bytes())?;
        swift_sources.push(p);
    }

    let swift_main = build_dir.join("WidgetsMain.swift");
    let main_source = generate_widgets_main_swift(&widget_cfg.widgets, &app_group);
    write_if_changed(&swift_main, main_source.as_bytes())?;

    let mut arch_binaries = Vec::new();
    for &arch in opts.arch.slices() {
        let runtime_dir = compile_swift_runtime(&build_dir, arch, &swift_sources)?;
        let out_bin = build_dir.join(format!("{executable}-{arch}"));
        let cache = build_dir.join("module-cache");
        fs::create_dir_all(&cache)?;
        let output = Command::new("xcrun")
            .args(["--sdk", "macosx", "swiftc"])
            .arg("-module-cache-path")
            .arg(&cache)
            .args([
                "-parse-as-library",
                "-target",
                &format!("{arch}-apple-macosx{WIDGET_MIN_MACOS}"),
                "-swift-version",
                "5",
                "-O",
                "-application-extension",
                "-Xlinker",
                "-dead_strip",
            ])
            .arg("-I")
            .arg(&runtime_dir)
            .arg("-L")
            .arg(&runtime_dir)
            .args([
                "-lNshWidgetRuntime",
                "-framework",
                "WidgetKit",
                "-framework",
                "SwiftUI",
                "-framework",
                "AppIntents",
            ])
            .arg("-o")
            .arg(&out_bin)
            .arg(&swift_main)
            .output()
            .context("run xcrun --sdk macosx swiftc for widget extension")?;
        if !output.status.success() {
            bail!(
                "compiling macOS widget extension ({arch}) failed:\n{}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let _ = Command::new("strip").arg(&out_bin).status();
        arch_binaries.push(out_bin);
    }

    if appex_path.exists() {
        fs::remove_dir_all(&appex_path)
            .with_context(|| format!("clean {}", appex_path.display()))?;
    }
    let contents = appex_path.join("Contents");
    let macos = contents.join("MacOS");
    let widgets_res = contents.join("Resources/nsh/widgets");
    fs::create_dir_all(&macos)?;
    fs::create_dir_all(&widgets_res)?;

    let exec_dest = macos.join(&executable);
    if arch_binaries.len() == 1 {
        fs::copy(&arch_binaries[0], &exec_dest)?;
    } else {
        let mut lipo = Command::new("lipo");
        lipo.arg("-create");
        for bin in &arch_binaries {
            lipo.arg(bin);
        }
        lipo.arg("-output").arg(&exec_dest);
        let out = lipo
            .output()
            .context("run lipo for universal widget extension")?;
        if !out.status.success() {
            bail!(
                "lipo widget extension failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = fs::metadata(&exec_dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exec_dest, perms)?;
    }

    // Compile default placeholder snapshots for `Contents/Resources/nsh/widgets/default-snapshots.json`.
    let compiler = match app_cfg
        .theme
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|rel| fs::read_to_string(app_dir.join(rel)).ok())
    {
        Some(json) => WidgetCompiler::with_tokens(
            &tokens_from_theme_json(&json).map_err(|e| anyhow::anyhow!("{e}"))?,
        ),
        None => WidgetCompiler::default(),
    };
    let ts = now_ms();
    let mut default_snapshots = BTreeMap::new();
    for entry in &widget_cfg.widgets {
        let snapshot = compile_default_snapshot(&compiler, entry, ts)?;
        default_snapshots.insert(entry.id.clone(), snapshot);
    }
    fs::write(
        widgets_res.join("default-snapshots.json"),
        serde_json::to_vec(&default_snapshots)?,
    )?;

    // Write `Contents/Info.plist`.
    let info = build_info_plist(&app_cfg, &bundle_id, &executable);
    plist::Value::Dictionary(info)
        .to_file_xml(contents.join("Info.plist"))
        .context("write widget extension Info.plist")?;

    Ok(BuildOutput {
        appex_path,
        bundle_id,
        app_group,
        widget_count: widget_cfg.widgets.len(),
    })
}

fn compile_default_snapshot(
    compiler: &WidgetCompiler,
    entry: &WidgetEntryConfig,
    now: i64,
) -> anyhow::Result<crate::ir::WidgetSnapshot> {
    let raw_tree: RawWidgetTree = match &entry.default_tree {
        Some(val) => serde_json::from_value(val.clone())
            .with_context(|| format!("invalid defaultTree for widget {:?}", entry.id))?,
        None => fallback_placeholder_tree(entry),
    };
    let mut families = BTreeMap::new();
    for &family in &entry.families {
        families.insert(family, raw_tree.clone());
    }
    let raw = RawWidgetSnapshot {
        widget_id: entry.id.clone(),
        generated_at_ms: Some(now),
        reload_policy: None,
        entries: vec![RawWidgetTimelineEntry {
            date_ms: now,
            relevance: None,
            families,
        }],
    };
    compiler
        .compile_snapshot(&raw, now)
        .map_err(|e| anyhow::anyhow!("compile defaultTree for widget {:?}: {e}", entry.id))
}

fn fallback_placeholder_tree(entry: &WidgetEntryConfig) -> RawWidgetTree {
    let mut children = vec![RawWidgetNode::Text {
        text: Some(entry.name.clone()),
        runs: Vec::new(),
        class: Some("text-sm font-semibold text-foreground".into()),
        style: None,
        number_of_lines: Some(1),
        widget: None,
    }];
    if !entry.description.trim().is_empty() {
        children.push(RawWidgetNode::Text {
            text: Some(entry.description.clone()),
            runs: Vec::new(),
            class: Some("text-xs text-muted-foreground".into()),
            style: None,
            number_of_lines: Some(2),
            widget: None,
        });
    }
    RawWidgetTree {
        url: None,
        root: RawWidgetNode::Box {
            axis: None,
            class: Some("flex-1 flex-col justify-between p-4 bg-card".into()),
            style: None,
            widget: None,
            children,
        },
    }
}

fn compile_swift_runtime(
    build_dir: &Path,
    arch: &str,
    sources: &[PathBuf],
) -> anyhow::Result<PathBuf> {
    let out = build_dir.join("runtime").join(arch);
    fs::create_dir_all(&out)?;
    let lib = out.join("libNshWidgetRuntime.a");
    let module = out.join("NshWidgetRuntime.swiftmodule");
    let newest_src = sources
        .iter()
        .filter_map(|p| fs::metadata(p).ok()?.modified().ok())
        .max();
    let built_mtime = fs::metadata(&lib).ok().and_then(|m| m.modified().ok());
    if lib.is_file()
        && module.is_file()
        && newest_src.is_none_or(|src| built_mtime.is_some_and(|dst| dst >= src))
    {
        return Ok(out);
    }

    let cache = out.join("module-cache");
    fs::create_dir_all(&cache)?;
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "swiftc"])
        .arg("-module-cache-path")
        .arg(&cache)
        .args([
            "-parse-as-library",
            "-module-name",
            "NshWidgetRuntime",
            "-target",
            &format!("{arch}-apple-macosx{WIDGET_MIN_MACOS}"),
            "-swift-version",
            "5",
            "-enable-library-evolution",
            "-emit-library",
            "-static",
            "-O",
            "-whole-module-optimization",
        ])
        .arg("-emit-module-path")
        .arg(&module)
        .arg("-emit-module-interface-path")
        .arg(out.join("NshWidgetRuntime.swiftinterface"))
        .arg("-o")
        .arg(&lib)
        .args(sources)
        .output()
        .context("run xcrun --sdk macosx swiftc for NshWidgetRuntime")?;
    if !output.status.success() {
        bail!(
            "compiling NshWidgetRuntime ({arch}) failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(out)
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn first_app_group(entitlements: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    entitlements
        .get("com.apple.security.application-groups")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::trim))
                .find(|s| !s.is_empty())
                .map(str::to_owned)
        })
}

fn build_info_plist(
    app_cfg: &AppNshConfig,
    bundle_id: &str,
    executable: &str,
) -> plist::Dictionary {
    use plist::Value;
    let s = |v: &str| Value::String(v.to_owned());
    let version = app_cfg.version.as_deref().unwrap_or("1.0.0");
    let build = match &app_cfg.build {
        Some(serde_json::Value::String(b)) => b.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => "1".to_owned(),
    };
    let min_sys = app_cfg
        .macos
        .minimum_system_version
        .as_deref()
        .filter(|v| version_at_least(v, WIDGET_MIN_MACOS))
        .unwrap_or(WIDGET_MIN_MACOS);
    let display_name = app_cfg
        .name
        .as_deref()
        .map(|n| format!("{n} Widgets"))
        .unwrap_or_else(|| executable.to_owned());

    let mut ext = plist::Dictionary::new();
    ext.insert(
        "NSExtensionPointIdentifier".into(),
        s("com.apple.widgetkit-extension"),
    );

    let mut d = plist::Dictionary::new();
    d.insert("CFBundleDevelopmentRegion".into(), s("en"));
    d.insert("CFBundleDisplayName".into(), s(&display_name));
    d.insert("CFBundleExecutable".into(), s(executable));
    d.insert("CFBundleIdentifier".into(), s(bundle_id));
    d.insert("CFBundleInfoDictionaryVersion".into(), s("6.0"));
    d.insert("CFBundleName".into(), s(executable));
    d.insert("CFBundlePackageType".into(), s("XPC!"));
    d.insert("CFBundleShortVersionString".into(), s(version));
    d.insert("CFBundleVersion".into(), s(&build));
    d.insert(
        "CFBundleSupportedPlatforms".into(),
        Value::Array(vec![s("MacOSX")]),
    );
    d.insert("LSMinimumSystemVersion".into(), s(min_sys));
    d.insert("NSExtension".into(), Value::Dictionary(ext));
    d
}

fn version_at_least(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    let len = va.len().max(vb.len());
    for i in 0..len {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    true
}

/// Generates `WidgetsMain.swift` declaring one SwiftUI `Widget` per entry in
/// `widgets` and the `@main` `WidgetBundle`.
pub fn generate_widgets_main_swift(widgets: &[WidgetEntryConfig], app_group: &str) -> String {
    let mut out = String::from(
        "// Generated by nsh-widget. Do not edit.\n\
         import Foundation\n\
         import SwiftUI\n\
         import WidgetKit\n\
         import NshWidgetRuntime\n\n",
    );
    let mut struct_names = Vec::with_capacity(widgets.len());
    for (idx, w) in widgets.iter().enumerate() {
        let name = format!("NshWidget_{}_{}", idx, sanitize_swift_ident(&w.id));
        let families: Vec<&str> = w.families.iter().map(|f| f.swift_case()).collect();
        out.push_str(&format!(
            "struct {name}: Widget {{\n\
             \x20   var body: some WidgetConfiguration {{\n\
             \x20       StaticConfiguration(\n\
             \x20           kind: {id},\n\
             \x20           provider: NshTimelineProvider(widgetId: {id}, appGroup: {group})\n\
             \x20       ) {{ entry in\n\
             \x20           NshWidgetRootView(entry: entry)\n\
             \x20       }}\n\
             \x20       .configurationDisplayName({display_name})\n\
             \x20       .description({description})\n\
             \x20       .supportedFamilies([{families}])\n\
             \x20   }}\n\
             }}\n\n",
            id = swift_string(&w.id),
            group = swift_string(app_group),
            display_name = swift_string(&w.name),
            description = swift_string(&w.description),
            families = families.join(", "),
        ));
        struct_names.push(name);
    }

    out.push_str(
        "@main\nstruct NshAppWidgetsBundle: WidgetBundle {\n    var body: some Widget {\n",
    );
    for name in &struct_names {
        out.push_str(&format!("        {name}()\n"));
    }
    out.push_str("    }\n}\n");
    out
}

fn sanitize_swift_ident(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("Item");
    }
    out
}

fn swift_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compiler::{RawWidgetSnapshot, WidgetCompiler},
        config::WidgetFamily,
    };

    #[test]
    fn generates_valid_widgets_main_swift() {
        let entry = WidgetEntryConfig {
            id: "portfolio".into(),
            name: "Portfolio".into(),
            description: "Watchlist \"live\" quotes".into(),
            families: vec![WidgetFamily::Small, WidgetFamily::Medium],
            default_tree: None,
        };
        let swift = generate_widgets_main_swift(&[entry], "ABCDE12345.com.example.pulse.widgets");
        assert!(swift.contains("import NshWidgetRuntime"));
        assert!(swift.contains("kind: \"portfolio\""));
        assert!(swift.contains("appGroup: \"ABCDE12345.com.example.pulse.widgets\""));
        assert!(swift.contains(".configurationDisplayName(\"Portfolio\")"));
        assert!(swift.contains(".description(\"Watchlist \\\"live\\\" quotes\")"));
        assert!(swift.contains(".supportedFamilies([.systemSmall, .systemMedium])"));
        assert!(swift.contains("@main\nstruct NshAppWidgetsBundle: WidgetBundle"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn compiled_widget_snapshot_decodes_cleanly_in_swift() {
        let compiler = WidgetCompiler::default();
        let raw: RawWidgetSnapshot = serde_json::from_value(serde_json::json!({
            "widgetId": "portfolio",
            "reloadPolicy": { "after": 900_000 },
            "entries": [{
                "dateMs": 1_795_280_000_000i64,
                "relevance": 0.85,
                "families": {
                    "medium": {
                        "url": "pulse://symbol/AAPL",
                        "root": {
                            "kind": "col",
                            "class": "flex-1 justify-between p-4 bg-card",
                            "children": [
                                {
                                    "kind": "text",
                                    "text": "AAPL",
                                    "class": "text-sm font-semibold font-mono text-foreground"
                                },
                                {
                                    "kind": "image",
                                    "source": "symbol:chart.line.uptrend.xyaxis",
                                    "class": "w-4 h-4",
                                    "style": { "tintColor": "#10b981" }
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
                                    "accessibilityLabel": "Open AAPL chart",
                                    "children": [{
                                        "kind": "date",
                                        "dateMs": 1_795_280_000_000i64,
                                        "mode": "relative",
                                        "class": "text-xs text-muted-foreground"
                                    }]
                                },
                                {
                                    "kind": "action",
                                    "action": "refresh",
                                    "payload": { "symbol": "AAPL" },
                                    "patch": { "stale": false },
                                    "accessibilityLabel": "Refresh",
                                    "children": [{
                                        "kind": "text",
                                        "text": "Refresh",
                                        "class": "text-xs font-medium text-secondary-foreground"
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
        let json = serde_json::to_string(&snapshot).unwrap();

        let dir = std::env::temp_dir().join(format!("nsh-widget-swift-ir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let json_path = dir.join("golden.json");
        fs::write(&json_path, &json).unwrap();

        let swift_main = dir.join("main.swift");
        fs::write(
            &swift_main,
            r#"
import Foundation

let url = URL(fileURLWithPath: CommandLine.arguments[1])
let data = try Data(contentsOf: url)
let snapshot = try JSONDecoder().decode(WidgetSnapshot.self, from: data)
precondition(snapshot.version == 1)
precondition(snapshot.widgetId == "portfolio")
precondition(snapshot.reloadAfterMs == 900_000)
precondition(snapshot.entries.count == 1)
let tree = snapshot.entries[0].families["medium"]!
precondition(tree.url == "pulse://symbol/AAPL")
precondition(tree.containerBackground != nil)
if case .box(let layout, _, let children) = tree.root {
    precondition(layout.axis == .column && layout.mainAlign == .spaceBetween)
    precondition(children.count == 5)
    if case .progress(let v, _, _, _, _) = children[2] {
        precondition(abs(v - 0.72) < 0.01)
    } else { fatalError("expected progress") }
    if case .link(let href, let label, .box(_, _, let linkChildren)) = children[3] {
        precondition(href == "pulse://symbol/AAPL/chart" && label == "Open AAPL chart")
        if case .date(let ts, let mode, _, _, _) = linkChildren.first {
            precondition(ts == 1_795_280_000_000 && mode == .relative)
        } else { fatalError("expected date inside link") }
    } else { fatalError("expected link") }
    if case .action(let act, let payload, let patch, _, _) = children[4] {
        precondition(act == "refresh" && payload?.contains("AAPL") == true && patch?.contains("stale") == true)
    } else { fatalError("expected action") }
} else {
    fatalError("expected root box")
}
let reencoded = try JSONEncoder().encode(snapshot)
let roundtrip = try JSONDecoder().decode(WidgetSnapshot.self, from: reencoded)
precondition(roundtrip == snapshot)
print("SWIFT_IR_OK")
"#,
        )
        .unwrap();

        let ir_swift = Path::new(env!("CARGO_MANIFEST_DIR")).join("swift/runtime/WidgetIR.swift");
        let bin = dir.join("verify_ir");
        let cache = dir.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let compile = Command::new("xcrun")
            .args(["--sdk", "macosx", "swiftc"])
            .arg("-module-cache-path")
            .arg(&cache)
            .args(["-swift-version", "5", "-o"])
            .arg(&bin)
            .arg(&ir_swift)
            .arg(&swift_main)
            .output()
            .expect("run swiftc");
        assert!(
            compile.status.success(),
            "swiftc failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&bin)
            .arg(&json_path)
            .output()
            .expect("run verify_ir");
        assert!(
            run.status.success(),
            "verify_ir failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(String::from_utf8_lossy(&run.stdout).contains("SWIFT_IR_OK"));
        let _ = fs::remove_dir_all(&dir);
    }
}
