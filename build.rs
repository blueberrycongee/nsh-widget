//! Compiles `swift/NshWidgetHostBridge.swift` on macOS so `nsh-widget`
//! can reload `WidgetCenter.shared` timelines, resolve App Group containers,
//! and auto-detect the signed `com.apple.security.application-groups`
//! entitlement without pulling WidgetKit into non-macOS builds.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=swift/NshWidgetHostBridge.swift");
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "macos" {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let cache_dir = out_dir.join("swift-module-cache");
    let _ = fs::create_dir_all(&cache_dir);

    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        Ok(other) => panic!("unsupported macOS arch: {other}"),
        Err(_) => "arm64",
    };
    let deployment = env::var("MACOSX_DEPLOYMENT_TARGET")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "13.0".to_owned());
    let target = format!("{arch}-apple-macosx{deployment}");

    let sdk_output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("run xcrun --sdk macosx --show-sdk-path");
    assert!(
        sdk_output.status.success(),
        "xcrun --sdk macosx --show-sdk-path failed: {}",
        String::from_utf8_lossy(&sdk_output.stderr)
    );
    let sdk = String::from_utf8(sdk_output.stdout)
        .expect("UTF-8 SDK path")
        .trim()
        .to_owned();

    let lib = out_dir.join("libnsh_widget_host_bridge.a");
    let status = Command::new("xcrun")
        .args(["--sdk", "macosx", "swiftc"])
        .arg("-module-cache-path")
        .arg(&cache_dir)
        .args([
            "-parse-as-library",
            "-module-name",
            "NshWidgetHostBridge",
            "-target",
            &target,
            "-sdk",
            &sdk,
            "-swift-version",
            "5",
            "-emit-library",
            "-static",
            "-O",
        ])
        .arg("swift/NshWidgetHostBridge.swift")
        .arg("-o")
        .arg(&lib)
        .status()
        .expect("run swiftc for NshWidgetHostBridge.swift");
    assert!(
        status.success(),
        "swiftc failed for NshWidgetHostBridge.swift"
    );

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=nsh_widget_host_bridge");
    println!("cargo:rustc-link-lib=framework=WidgetKit");
    println!("cargo:rustc-link-lib=framework=Security");
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
}
