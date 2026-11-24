import AppKit
import SwiftUI
import WidgetKit

/// Root SwiftUI view interpreting a `NshWidgetEntry` for the active
/// `WidgetFamily`, lifting `containerBackground` to `.containerBackground(for: .widget)`
/// and supporting macOS 14/15 Light, Dark, Tinted, and Desktop Unfocused modes.
@available(macOS 14.0, *)
public struct NshWidgetRootView: View {
    @Environment(\.widgetFamily) private var family
    private let entry: NshWidgetEntry

    public init(entry: NshWidgetEntry) {
        self.entry = entry
    }

    private var selectedTree: WidgetFamilyTree? {
        let key: String
        switch family {
        case .systemSmall:
            key = "small"
        case .systemMedium:
            key = "medium"
        case .systemLarge:
            key = "large"
        default:
            key = "medium"
        }
        return entry.families[key]
            ?? entry.families["medium"]
            ?? entry.families["small"]
            ?? entry.families.values.first
    }

    public var body: some View {
        let tree = selectedTree
        let store = NshWidgetStore(appGroup: entry.appGroup)
        Group {
            if let tree {
                WidgetNodeView(
                    node: tree.root,
                    widgetId: entry.widgetId,
                    appGroup: entry.appGroup,
                    store: store
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else {
                Color.clear
            }
        }
        .redacted(reason: entry.isPlaceholder ? .placeholder : [])
        .modifier(OptionalWidgetURL(urlString: tree?.url))
        .containerBackground(for: .widget) {
            if let bg = tree?.containerBackground {
                WidgetFillView(fill: bg)
            } else {
                Color.clear
            }
        }
    }
}

@available(macOS 14.0, *)
private struct OptionalWidgetURL: ViewModifier {
    let urlString: String?

    func body(content: Content) -> some View {
        if let urlString, let url = URL(string: urlString) {
            content.widgetURL(url)
        } else {
            content
        }
    }
}

@available(macOS 14.0, *)
private struct WidgetFillView: View {
    let fill: WidgetFill

    var body: some View {
        switch fill {
        case .solid(let color):
            color.swiftUIColor()
        case .linearGradient(let angleDeg, let stops):
            let (start, end) = unitPoints(forCssAngleDeg: angleDeg)
            LinearGradient(
                gradient: Gradient(stops: stops.map {
                    Gradient.Stop(color: $0.color.swiftUIColor(), location: $0.position)
                }),
                startPoint: start,
                endPoint: end
            )
        }
    }

    /// Converts CSS linear-gradient angle (0° = up, 90° = right, 180° = down)
    /// into SwiftUI `UnitPoint` endpoints.
    private func unitPoints(forCssAngleDeg deg: Double) -> (UnitPoint, UnitPoint) {
        let rad = deg * .pi / 180.0
        let dx = sin(rad)
        let dy = -cos(rad)
        let start = UnitPoint(x: 0.5 - dx * 0.5, y: 0.5 - dy * 0.5)
        let end = UnitPoint(x: 0.5 + dx * 0.5, y: 0.5 + dy * 0.5)
        return (start, end)
    }
}

@available(macOS 14.0, *)
private struct WidgetNodeView: View {
    let node: WidgetNode
    let widgetId: String
    let appGroup: String
    let store: NshWidgetStore

    var body: some View {
        switch node {
        case .box(let layout, let style, let children):
            renderBox(layout: layout, style: style, children: children)
        case .text(let runs, let align, let lineLimit, let accentable, let layout):
            renderText(
                runs: runs,
                align: align,
                lineLimit: lineLimit,
                accentable: accentable,
                layout: layout
            )
        case .date(let timestampMs, let mode, let style, let accentable, let layout):
            renderDate(
                timestampMs: timestampMs,
                mode: mode,
                style: style,
                accentable: accentable,
                layout: layout
            )
        case .image(let source, let resizeMode, let tint, let accentedMode, let style, let layout):
            renderImage(
                source: source,
                resizeMode: resizeMode,
                tint: tint,
                accentedMode: accentedMode,
                style: style,
                layout: layout
            )
        case .progress(let value, let trackColor, let indicatorColor, let cornerRadius, let layout):
            renderProgress(
                value: value,
                trackColor: trackColor,
                indicatorColor: indicatorColor,
                cornerRadius: cornerRadius,
                layout: layout
            )
        case .link(let href, let accessibilityLabel, let child):
            if let url = URL(string: href) {
                Link(destination: url) {
                    WidgetNodeView(node: child, widgetId: widgetId, appGroup: appGroup, store: store)
                }
                .modifier(OptionalAccessibilityLabel(label: accessibilityLabel))
            } else {
                WidgetNodeView(node: child, widgetId: widgetId, appGroup: appGroup, store: store)
            }
        case .action(let action, let payloadJson, let patchJson, let accessibilityLabel, let child):
            Button(
                intent: NshWidgetActionIntent(
                    widgetId: widgetId,
                    appGroup: appGroup,
                    action: action,
                    payloadJson: payloadJson,
                    patchJson: patchJson
                )
            ) {
                WidgetNodeView(node: child, widgetId: widgetId, appGroup: appGroup, store: store)
            }
            .buttonStyle(.plain)
            .modifier(OptionalAccessibilityLabel(label: accessibilityLabel))
        }
    }

    @ViewBuilder
    private func renderBox(
        layout: WidgetLayout,
        style: WidgetBoxStyle,
        children: [WidgetNode]
    ) -> some View {
        let content = Group {
            switch layout.axis {
            case .row:
                HStack(alignment: verticalAlignment(layout.crossAlign), spacing: layout.gap) {
                    renderStackChildren(children: children, mainAlign: layout.mainAlign)
                }
            case .column:
                VStack(alignment: horizontalAlignment(layout.crossAlign), spacing: layout.gap) {
                    renderStackChildren(children: children, mainAlign: layout.mainAlign)
                }
            case .stack:
                ZStack(alignment: stackAlignment(layout.mainAlign, layout.crossAlign)) {
                    ForEach(Array(children.enumerated()), id: \.offset) { _, child in
                        WidgetNodeView(node: child, widgetId: widgetId, appGroup: appGroup, store: store)
                    }
                }
            }
        }
        content
            .padding(
                EdgeInsets(
                    top: layout.padding.top,
                    leading: layout.padding.left,
                    bottom: layout.padding.bottom,
                    trailing: layout.padding.right
                )
            )
            .modifier(WidgetLayoutModifier(layout: layout))
            .modifier(WidgetBoxStyleModifier(style: style))
    }

    @ViewBuilder
    private func renderStackChildren(children: [WidgetNode], mainAlign: WidgetMainAlign) -> some View {
        if mainAlign == .center || mainAlign == .end || mainAlign == .spaceAround || mainAlign == .spaceEvenly {
            Spacer(minLength: 0)
        }
        ForEach(Array(children.enumerated()), id: \.offset) { idx, child in
            if idx > 0 && (mainAlign == .spaceBetween || mainAlign == .spaceAround || mainAlign == .spaceEvenly) {
                Spacer(minLength: 0)
            }
            WidgetNodeView(node: child, widgetId: widgetId, appGroup: appGroup, store: store)
        }
        if mainAlign == .center || mainAlign == .spaceAround || mainAlign == .spaceEvenly {
            Spacer(minLength: 0)
        }
    }

    private func renderText(
        runs: [WidgetTextRun],
        align: WidgetTextAlign,
        lineLimit: Int?,
        accentable: Bool,
        layout: WidgetLayout
    ) -> some View {
        let combined: Text = runs.reduce(Text("")) { acc, run in
            acc + styledText(Text(run.text), style: run.style)
        }
        return combined
            .multilineTextAlignment(textAlignment(align))
            .lineLimit(lineLimit)
            .widgetAccentable(accentable)
            .modifier(WidgetLayoutModifier(layout: layout))
    }

    private func renderDate(
        timestampMs: Int64,
        mode: WidgetDateMode,
        style: WidgetTextStyle,
        accentable: Bool,
        layout: WidgetLayout
    ) -> some View {
        let date = Date(timeIntervalSince1970: Double(timestampMs) / 1000.0)
        let base: Text
        switch mode {
        case .relative:
            base = Text(date, style: .relative)
        case .timer:
            base = Text(date, style: .timer)
        case .offset:
            base = Text(date, style: .offset)
        case .time:
            base = Text(date, style: .time)
        case .date:
            base = Text(date, style: .date)
        }
        return styledText(base, style: style)
            .widgetAccentable(accentable)
            .modifier(WidgetLayoutModifier(layout: layout))
    }

    @ViewBuilder
    private func renderImage(
        source: WidgetImageSource,
        resizeMode: WidgetResizeMode,
        tint: WidgetColor?,
        accentedMode: WidgetAccentedImageMode,
        style: WidgetBoxStyle,
        layout: WidgetLayout
    ) -> some View {
        let image: Image? = {
            switch source {
            case .symbol(let name):
                return Image(systemName: name)
            case .asset, .file:
                if let nsImage = store.loadImage(source: source, widgetId: widgetId) {
                    return Image(nsImage: nsImage)
                }
                return nil
            }
        }()

        if let image {
            let isSymbol: Bool = {
                if case .symbol = source { return true }
                return false
            }()
            let resizable = (isSymbol || resizeMode != .center) ? image.resizable() : image
            let accented = applyAccentedImageMode(resizable, mode: accentedMode)
            let sized = applyImageAspect(accented, mode: resizeMode, isSymbol: isSymbol)
            sized
                .modifier(OptionalForegroundColor(color: tint))
                .modifier(WidgetLayoutModifier(layout: layout))
                .modifier(WidgetBoxStyleModifier(style: style))
        } else {
            Color.clear
                .modifier(WidgetLayoutModifier(layout: layout))
                .modifier(WidgetBoxStyleModifier(style: style))
        }
    }

    @ViewBuilder
    private func applyAccentedImageMode(_ image: Image, mode: WidgetAccentedImageMode) -> some View {
        if #available(macOS 15.0, *) {
            switch mode {
            case .fullColor:
                image.widgetAccentedRenderingMode(.fullColor)
            case .accented:
                image.widgetAccentedRenderingMode(.accented)
            case .desaturated:
                image.widgetAccentedRenderingMode(.accentedDesaturated)
            case .template:
                image.widgetAccentedRenderingMode(.desaturated)
            }
        } else {
            image
        }
    }

    @ViewBuilder
    private func applyImageAspect<V: View>(_ view: V, mode: WidgetResizeMode, isSymbol: Bool) -> some View {
        if isSymbol {
            view.aspectRatio(contentMode: mode == .cover ? .fill : .fit)
        } else {
            switch mode {
            case .contain:
                view.aspectRatio(contentMode: .fit)
            case .cover:
                view.aspectRatio(contentMode: .fill)
            case .stretch, .center:
                view
            }
        }
    }

    private func renderProgress(
        value: Double,
        trackColor: WidgetColor?,
        indicatorColor: WidgetColor?,
        cornerRadius: Double?,
        layout: WidgetLayout
    ) -> some View {
        let clamped = max(0.0, min(1.0, value))
        let height = layout.height ?? 6.0
        let radius = cornerRadius ?? (height / 2.0)
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(trackColor?.swiftUIColor() ?? Color.primary.opacity(0.14))
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(indicatorColor?.swiftUIColor() ?? Color.accentColor)
                    .frame(width: max(0, geo.size.width * clamped))
                    .widgetAccentable(true)
            }
        }
        .frame(height: height)
        .modifier(WidgetLayoutModifier(layout: layout))
    }

    private func styledText(_ text: Text, style: WidgetTextStyle) -> Text {
        let design: Font.Design = style.monospaced ? .monospaced : .default
        var out = text.font(.system(size: style.fontSize, weight: fontWeight(style.fontWeight), design: design))
        if style.italic {
            out = out.italic()
        }
        if let color = style.color {
            out = out.foregroundColor(color.swiftUIColor())
        }
        if style.underline {
            out = out.underline()
        }
        if style.strikethrough {
            out = out.strikethrough()
        }
        if let spacing = style.letterSpacing {
            out = out.tracking(spacing)
        }
        return out
    }

    private func fontWeight(_ weight: UInt16) -> Font.Weight {
        switch weight {
        case ..<150: return .ultraLight
        case 150..<250: return .thin
        case 250..<350: return .light
        case 350..<450: return .regular
        case 450..<550: return .medium
        case 550..<650: return .semibold
        case 650..<750: return .bold
        case 750..<850: return .heavy
        default: return .black
        }
    }

    private func textAlignment(_ align: WidgetTextAlign) -> TextAlignment {
        switch align {
        case .leading: return .leading
        case .center: return .center
        case .trailing: return .trailing
        }
    }

    private func horizontalAlignment(_ cross: WidgetCrossAlign) -> HorizontalAlignment {
        switch cross {
        case .start, .stretch: return .leading
        case .center: return .center
        case .end: return .trailing
        }
    }

    private func verticalAlignment(_ cross: WidgetCrossAlign) -> VerticalAlignment {
        switch cross {
        case .start, .stretch: return .top
        case .center: return .center
        case .end: return .bottom
        }
    }

    private func stackAlignment(_ main: WidgetMainAlign, _ cross: WidgetCrossAlign) -> Alignment {
        let h: HorizontalAlignment = horizontalAlignment(cross)
        let v: VerticalAlignment
        switch main {
        case .center: v = .center
        case .end: v = .bottom
        default: v = .top
        }
        return Alignment(horizontal: h, vertical: v)
    }
}

@available(macOS 14.0, *)
private struct WidgetLayoutModifier: ViewModifier {
    let layout: WidgetLayout

    func body(content: Content) -> some View {
        let maxW: CGFloat? = layout.maxWidth.map { CGFloat($0) }
            ?? ((layout.flexGrow > 0 || layout.mainAlign == .spaceBetween || layout.crossAlign == .stretch) ? .infinity : nil)
        let maxH: CGFloat? = layout.maxHeight.map { CGFloat($0) }
            ?? (layout.flexGrow > 0 ? .infinity : nil)
        let align: Alignment = {
            switch layout.crossAlign {
            case .center: return .center
            case .end: return .trailing
            default: return .leading
            }
        }()
        if layout.width != nil || layout.height != nil {
            content.frame(
                width: layout.width.map { CGFloat($0) },
                height: layout.height.map { CGFloat($0) },
                alignment: align
            )
        } else if layout.minWidth != nil || layout.minHeight != nil || maxW != nil || maxH != nil {
            content.frame(
                minWidth: layout.minWidth.map { CGFloat($0) },
                maxWidth: maxW,
                minHeight: layout.minHeight.map { CGFloat($0) },
                maxHeight: maxH,
                alignment: align
            )
        } else {
            content
        }
    }
}

@available(macOS 14.0, *)
private struct WidgetBoxStyleModifier: ViewModifier {
    let style: WidgetBoxStyle

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: style.cornerRadius, style: .continuous)
        content
            .background {
                if let bg = style.background {
                    WidgetFillView(fill: bg)
                        .clipShape(shape)
                }
            }
            .overlay {
                if let borderColor = style.borderColor, style.borderWidth > 0 {
                    shape.strokeBorder(borderColor.swiftUIColor(), lineWidth: style.borderWidth)
                }
            }
            .modifier(OptionalClipShape(clip: style.clip || style.cornerRadius > 0, radius: style.cornerRadius))
            .opacity(style.opacity)
            .widgetAccentable(style.accentable)
    }
}

@available(macOS 14.0, *)
private struct OptionalClipShape: ViewModifier {
    let clip: Bool
    let radius: Double

    func body(content: Content) -> some View {
        if clip {
            content.clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
        } else {
            content
        }
    }
}

@available(macOS 14.0, *)
private struct OptionalForegroundColor: ViewModifier {
    let color: WidgetColor?

    func body(content: Content) -> some View {
        if let color {
            content.foregroundStyle(color.swiftUIColor())
        } else {
            content
        }
    }
}

@available(macOS 14.0, *)
private struct OptionalAccessibilityLabel: ViewModifier {
    let label: String?

    func body(content: Content) -> some View {
        if let label, !label.isEmpty {
            content.accessibilityLabel(label)
        } else {
            content
        }
    }
}
