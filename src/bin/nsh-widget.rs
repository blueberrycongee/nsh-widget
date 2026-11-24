//! CLI tool for building macOS WidgetKit `.appex` bundles without Xcode.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use nsh_widget::appex::{BuildOptions, TargetArch, build_macos_appex};

#[derive(Parser)]
#[command(
    name = "nsh-widget",
    about = "Build Apple WidgetKit .appex extensions for Native Shell apps without Xcode"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile and assemble a macOS WidgetKit `.appex` bundle.
    Build {
        /// App directory containing `nsh.json` and `nsh-widget.json`.
        #[arg(long, default_value = ".")]
        app: PathBuf,
        /// Optional explicit path to `nsh-widget.json`.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Optional output `.appex` path (defaults to `nsh-widget.json` `output`, `nsh.json` `macos.extensions[0].path`, or `macos/build/Widgets.appex`).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Target architecture: arm64, x86_64, or universal.
        #[arg(long, default_value = if cfg!(target_arch = "aarch64") { "arm64" } else { "x86_64" })]
        arch: String,
        /// Override the shared App Group identifier.
        #[arg(long)]
        app_group: Option<String>,
        /// Override the extension's CFBundleIdentifier (must start with `<appId>.`).
        #[arg(long)]
        bundle_id: Option<String>,
        /// Override the extension executable name (without `.appex`).
        #[arg(long)]
        executable: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build {
            app,
            config,
            out,
            arch,
            app_group,
            bundle_id,
            executable,
        } => {
            let arch = TargetArch::parse(&arch)?;
            let built = build_macos_appex(BuildOptions {
                app_dir: app,
                config_path: config,
                output_path: out,
                arch,
                app_group,
                bundle_id,
                executable,
            })?;
            println!(
                "Built {} ({} widget(s), bundleId={}, appGroup={})",
                built.appex_path.display(),
                built.widget_count,
                built.bundle_id,
                built.app_group
            );
        }
    }
    Ok(())
}
