# nsh-widget

Native Apple **WidgetKit** module (`nshm`) for [Native Shell](https://github.com/longbridge/nsh) — zero NSH core modifications required.

- **Hybrid Rust + TSX `nshm`**:
  - Rust native bridge (`nsh-widget/native`) compiling `Swift` `WidgetCenter.shared` + App Group container resolution + `SecCodeCopySelf` auto-detection + Darwin notification observation (`<appGroup>.action`).
  - **Dual-appearance Tailwind v4 + design-token compiler** powered by `nsh-core`'s own `nsh_core::tailwind`, resolving Light & Dark appearances in Rust into a compact `WidgetSnapshot` v1 IR.
  - **Declarative TSX & builder DSL** (`<WBox>`, `<WRow>`, `<WCol>`, `<WStack>`, `<WText>`, `<WDate>`, `<WImage>`, `<WProgress>`, `<WLink>`, `<WAction>`) exported from `"nsh-widget"` with full `nsh check` (`tsc`) and Tailwind class validation.
  - **Pure-TSX `<WidgetPreview>`** component for instant local Light / Dark / Tinted preview inside your NSH app.
  - **Zero-Xcode `.appex` builder** (`nsh-widget build`) compiling SwiftUI `NshWidgetRuntime` directly via `xcrun swiftc` and integrating with `nsh package --target macos` via `macos.extensions`.

## Requirements

- **Native Shell** `nightly` (or the next release that includes [longbridge/nsh#2878](https://github.com/longbridge/nsh/pull/2878) and [longbridge/nsh#2879](https://github.com/longbridge/nsh/pull/2879)).
- **macOS** with Xcode Command Line Tools (`xcrun swiftc`) for building the `.appex` WidgetKit extension.

## Installation

Add `nsh-widget` to your Native Shell app:

```sh
nsh add github:blueberrycongee/nsh-widget
# or locally during development:
nsh add --path ../nsh-widget
```

## Quick Start

### 1. Declare widgets in `nsh-widget.json`

```json
{
  "widgets": [
    {
      "id": "portfolio",
      "name": "Portfolio",
      "description": "Watchlist price, daily change, and goal progress.",
      "families": ["small", "medium", "large"]
    }
  ]
}
```

### 2. Build the macOS `.appex` extension (zero Xcode project)

Install the `nsh-widget` CLI:

```sh
cargo install --git https://github.com/blueberrycongee/nsh-widget --features cli
```

Then build the extension from your app directory:

```sh
nsh-widget build --app .
```

This compiles a pure-SwiftUI `macos/build/Widgets.appex` using `xcrun --sdk macosx swiftc`.

### 3. Configure `nsh.json` and package

```json
{
  "id": "com.example.pulse",
  "macos": {
    "entitlements": {
      "com.apple.security.application-groups": ["TEAMID.com.example.pulse.widgets"]
    },
    "extensions": [
      {
        "path": "macos/build/Widgets.appex",
        "entitlements": {
          "com.apple.security.app-sandbox": true,
          "com.apple.security.application-groups": ["TEAMID.com.example.pulse.widgets"]
        }
      }
    ]
  }
}
```

Package and sign (inside-out with Hardened Runtime):

```sh
nsh package --target macos --sign "Developer ID Application: Your Name (TEAMID)"
# or local ad-hoc verification:
nsh package --target macos
```

### 4. Write and update widgets in TSX

```tsx
import { defineWidget, WCol, WRow, WText, WProgress, WAction, Widget } from "nsh-widget";

export const PortfolioWidget = defineWidget({
  id: "portfolio",
  families: ["small", "medium", "large"],
  placeholder: { symbol: "AAPL", price: "$218.40", progress: 0.72 },
  render: ({ data, family }) => (
    <WCol class="flex-1 justify-between p-4 bg-card">
      <WRow class="items-center justify-between">
        <WText class="text-sm font-semibold text-foreground">{data.symbol}</WText>
        <WAction action="refresh" class="px-2 py-1 rounded-md bg-secondary">
          <WText class="text-xs text-secondary-foreground">Refresh</WText>
        </WAction>
      </WRow>
      <WText class="text-2xl font-bold text-foreground">{data.price}</WText>
      {family !== "small" && (
        <WProgress value={data.progress * 100} class="h-1.5 rounded-full bg-muted" indicatorClass="bg-primary" />
      )}
    </WCol>
  ),
});

if (await Widget.available()) {
  await Widget.update(PortfolioWidget, { symbol: "AAPL", price: "$218.40", progress: 0.72 });
}

Widget.on("action", (event) => {
  console.log("Widget button clicked:", event.widgetId, event.action, event.payload);
});
```

## Verification

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo run --features cli --bin nsh-widget -- build --app examples/pulse
nsh check examples/pulse
nsh package examples/pulse --target macos
codesign --verify --deep --strict --verbose=2 examples/pulse/.nsh/out/Pulse.app
```
