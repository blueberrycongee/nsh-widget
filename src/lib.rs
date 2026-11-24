//! Native Apple WidgetKit module (`nshm`) for Native Shell.
//!
//! Exports the native `Widget` module under `"nsh-widget/native"` (wrapped by
//! the `"nsh-widget"` TSX export in `ts/index.tsx`), a dual-appearance Tailwind
//! v4 → `WidgetSnapshot` IR compiler, an atomic App Group store, and (with the
//! `cli` feature) a zero-Xcode `.appex` builder.

pub mod compiler;
pub mod config;
pub mod host;
pub mod ir;
mod module;
pub mod store;

#[cfg(feature = "cli")]
pub mod appex;

use nsh_core::prelude::*;

#[native_module("nsh-widget/native")]
fn native(registry: &mut Registry) {
    registry.module::<module::Widget>();
}
