import AppKit
import Foundation
import SwiftUI

/// Current Widget Tree IR wire version matching `nsh_core::widget::WIDGET_IR_VERSION`.
public let nshWidgetIrVersion: UInt32 = 1

/// Serialized snapshot for one widget, containing ordered timeline entries
/// with pre-evaluated trees per `WidgetFamily`.
public struct WidgetSnapshot: Codable, Sendable, Equatable {
    public var version: UInt32
    public var widgetId: String
    public var generatedAtMs: Int64
    public var reloadAfterMs: Int64?
    public var reloadNever: Bool
    public var entries: [WidgetTimelineIrEntry]

    public init(
        version: UInt32 = nshWidgetIrVersion,
        widgetId: String,
        generatedAtMs: Int64,
        reloadAfterMs: Int64? = nil,
        reloadNever: Bool = false,
        entries: [WidgetTimelineIrEntry]
    ) {
        self.version = version
        self.widgetId = widgetId
        self.generatedAtMs = generatedAtMs
        self.reloadAfterMs = reloadAfterMs
        self.reloadNever = reloadNever
        self.entries = entries
    }

    enum CodingKeys: String, CodingKey {
        case version, widgetId, generatedAtMs, reloadAfterMs, reloadNever, entries
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        version = try c.decode(UInt32.self, forKey: .version)
        widgetId = try c.decode(String.self, forKey: .widgetId)
        generatedAtMs = try c.decode(Int64.self, forKey: .generatedAtMs)
        reloadAfterMs = try c.decodeIfPresent(Int64.self, forKey: .reloadAfterMs)
        reloadNever = try c.decodeIfPresent(Bool.self, forKey: .reloadNever) ?? false
        entries = try c.decode([WidgetTimelineIrEntry].self, forKey: .entries)
    }
}

/// One scheduled timeline entry inside a `WidgetSnapshot`.
public struct WidgetTimelineIrEntry: Codable, Sendable, Equatable {
    public var dateMs: Int64
    public var relevance: Double?
    public var families: [String: WidgetFamilyTree]

    public init(
        dateMs: Int64,
        relevance: Double? = nil,
        families: [String: WidgetFamilyTree]
    ) {
        self.dateMs = dateMs
        self.relevance = relevance
        self.families = families
    }
}

/// Evaluated tree for one widget family (`"small"`, `"medium"`, `"large"`).
public struct WidgetFamilyTree: Codable, Sendable, Equatable {
    public var url: String?
    public var containerBackground: WidgetFill?
    public var root: WidgetNode

    public init(
        url: String? = nil,
        containerBackground: WidgetFill? = nil,
        root: WidgetNode
    ) {
        self.url = url
        self.containerBackground = containerBackground
        self.root = root
    }
}

/// Dual-appearance sRGB `[r, g, b, a]` color (`0.0...1.0`) adapting
/// instantaneously to macOS Light and Dark appearances.
public struct WidgetColor: Codable, Sendable, Equatable {
    public var light: [Double]
    public var dark: [Double]

    public init(light: [Double], dark: [Double]) {
        self.light = light
        self.dark = dark
    }

    /// Dynamic `SwiftUI.Color` resolving against the live AppKit appearance.
    public func swiftUIColor() -> Color {
        let light = self.light
        let dark = self.dark
        let nsColor = NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            let rgba = isDark ? dark : light
            let r = rgba.count > 0 ? rgba[0] : 0
            let g = rgba.count > 1 ? rgba[1] : 0
            let b = rgba.count > 2 ? rgba[2] : 0
            let a = rgba.count > 3 ? rgba[3] : 1
            return NSColor(srgbRed: r, green: g, blue: b, alpha: a)
        }
        return Color(nsColor: nsColor)
    }
}

/// One color stop of a `WidgetFill.linearGradient`.
public struct WidgetGradientStop: Codable, Sendable, Equatable {
    public var color: WidgetColor
    public var position: Double

    public init(color: WidgetColor, position: Double) {
        self.color = color
        self.position = position
    }
}

/// Fill paint for widget backgrounds and `.containerBackground`.
public enum WidgetFill: Codable, Sendable, Equatable {
    case solid(color: WidgetColor)
    case linearGradient(angleDeg: Double, stops: [WidgetGradientStop])

    enum CodingKeys: String, CodingKey {
        case kind, color, angleDeg, stops
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "solid":
            self = .solid(color: try c.decode(WidgetColor.self, forKey: .color))
        case "linearGradient":
            self = .linearGradient(
                angleDeg: try c.decode(Double.self, forKey: .angleDeg),
                stops: try c.decode([WidgetGradientStop].self, forKey: .stops)
            )
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind,
                in: c,
                debugDescription: "Unknown WidgetFill kind: \(kind)"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .solid(let color):
            try c.encode("solid", forKey: .kind)
            try c.encode(color, forKey: .color)
        case .linearGradient(let angleDeg, let stops):
            try c.encode("linearGradient", forKey: .kind)
            try c.encode(angleDeg, forKey: .angleDeg)
            try c.encode(stops, forKey: .stops)
        }
    }
}

public enum WidgetAxis: String, Codable, Sendable, Equatable {
    case row, column, stack
}

public enum WidgetMainAlign: String, Codable, Sendable, Equatable {
    case start, center, end, spaceBetween, spaceAround, spaceEvenly
}

public enum WidgetCrossAlign: String, Codable, Sendable, Equatable {
    case start, center, end, stretch
}

public struct WidgetEdges: Codable, Sendable, Equatable {
    public var top: Double
    public var right: Double
    public var bottom: Double
    public var left: Double

    public init(top: Double = 0, right: Double = 0, bottom: Double = 0, left: Double = 0) {
        self.top = top
        self.right = right
        self.bottom = bottom
        self.left = left
    }

    enum CodingKeys: String, CodingKey {
        case top, right, bottom, left
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        top = try c.decodeIfPresent(Double.self, forKey: .top) ?? 0
        right = try c.decodeIfPresent(Double.self, forKey: .right) ?? 0
        bottom = try c.decodeIfPresent(Double.self, forKey: .bottom) ?? 0
        left = try c.decodeIfPresent(Double.self, forKey: .left) ?? 0
    }
}

public struct WidgetLayout: Codable, Sendable, Equatable {
    public var axis: WidgetAxis
    public var mainAlign: WidgetMainAlign
    public var crossAlign: WidgetCrossAlign
    public var gap: Double
    public var padding: WidgetEdges
    public var width: Double?
    public var height: Double?
    public var minWidth: Double?
    public var minHeight: Double?
    public var maxWidth: Double?
    public var maxHeight: Double?
    public var flexGrow: Double

    public init(
        axis: WidgetAxis = .column,
        mainAlign: WidgetMainAlign = .start,
        crossAlign: WidgetCrossAlign = .start,
        gap: Double = 0,
        padding: WidgetEdges = WidgetEdges(),
        width: Double? = nil,
        height: Double? = nil,
        minWidth: Double? = nil,
        minHeight: Double? = nil,
        maxWidth: Double? = nil,
        maxHeight: Double? = nil,
        flexGrow: Double = 0
    ) {
        self.axis = axis
        self.mainAlign = mainAlign
        self.crossAlign = crossAlign
        self.gap = gap
        self.padding = padding
        self.width = width
        self.height = height
        self.minWidth = minWidth
        self.minHeight = minHeight
        self.maxWidth = maxWidth
        self.maxHeight = maxHeight
        self.flexGrow = flexGrow
    }

    enum CodingKeys: String, CodingKey {
        case axis, mainAlign, crossAlign, gap, padding
        case width, height, minWidth, minHeight, maxWidth, maxHeight, flexGrow
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        axis = try c.decodeIfPresent(WidgetAxis.self, forKey: .axis) ?? .column
        mainAlign = try c.decodeIfPresent(WidgetMainAlign.self, forKey: .mainAlign) ?? .start
        crossAlign = try c.decodeIfPresent(WidgetCrossAlign.self, forKey: .crossAlign) ?? .start
        gap = try c.decodeIfPresent(Double.self, forKey: .gap) ?? 0
        padding = try c.decodeIfPresent(WidgetEdges.self, forKey: .padding) ?? WidgetEdges()
        width = try c.decodeIfPresent(Double.self, forKey: .width)
        height = try c.decodeIfPresent(Double.self, forKey: .height)
        minWidth = try c.decodeIfPresent(Double.self, forKey: .minWidth)
        minHeight = try c.decodeIfPresent(Double.self, forKey: .minHeight)
        maxWidth = try c.decodeIfPresent(Double.self, forKey: .maxWidth)
        maxHeight = try c.decodeIfPresent(Double.self, forKey: .maxHeight)
        flexGrow = try c.decodeIfPresent(Double.self, forKey: .flexGrow) ?? 0
    }
}

public struct WidgetBoxStyle: Codable, Sendable, Equatable {
    public var background: WidgetFill?
    public var borderColor: WidgetColor?
    public var borderWidth: Double
    public var cornerRadius: Double
    public var opacity: Double
    public var clip: Bool
    public var accentable: Bool

    public init(
        background: WidgetFill? = nil,
        borderColor: WidgetColor? = nil,
        borderWidth: Double = 0,
        cornerRadius: Double = 0,
        opacity: Double = 1,
        clip: Bool = false,
        accentable: Bool = false
    ) {
        self.background = background
        self.borderColor = borderColor
        self.borderWidth = borderWidth
        self.cornerRadius = cornerRadius
        self.opacity = opacity
        self.clip = clip
        self.accentable = accentable
    }

    enum CodingKeys: String, CodingKey {
        case background, borderColor, borderWidth, cornerRadius, opacity, clip, accentable
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        background = try c.decodeIfPresent(WidgetFill.self, forKey: .background)
        borderColor = try c.decodeIfPresent(WidgetColor.self, forKey: .borderColor)
        borderWidth = try c.decodeIfPresent(Double.self, forKey: .borderWidth) ?? 0
        cornerRadius = try c.decodeIfPresent(Double.self, forKey: .cornerRadius) ?? 0
        opacity = try c.decodeIfPresent(Double.self, forKey: .opacity) ?? 1
        clip = try c.decodeIfPresent(Bool.self, forKey: .clip) ?? false
        accentable = try c.decodeIfPresent(Bool.self, forKey: .accentable) ?? false
    }
}

public enum WidgetTextAlign: String, Codable, Sendable, Equatable {
    case leading, center, trailing
}

public struct WidgetTextStyle: Codable, Sendable, Equatable {
    public var fontSize: Double
    public var fontWeight: UInt16
    public var italic: Bool
    public var monospaced: Bool
    public var color: WidgetColor?
    public var underline: Bool
    public var strikethrough: Bool
    public var letterSpacing: Double?

    public init(
        fontSize: Double = 14,
        fontWeight: UInt16 = 400,
        italic: Bool = false,
        monospaced: Bool = false,
        color: WidgetColor? = nil,
        underline: Bool = false,
        strikethrough: Bool = false,
        letterSpacing: Double? = nil
    ) {
        self.fontSize = fontSize
        self.fontWeight = fontWeight
        self.italic = italic
        self.monospaced = monospaced
        self.color = color
        self.underline = underline
        self.strikethrough = strikethrough
        self.letterSpacing = letterSpacing
    }

    enum CodingKeys: String, CodingKey {
        case fontSize, fontWeight, italic, monospaced, color, underline, strikethrough, letterSpacing
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        fontSize = try c.decodeIfPresent(Double.self, forKey: .fontSize) ?? 14
        fontWeight = try c.decodeIfPresent(UInt16.self, forKey: .fontWeight) ?? 400
        italic = try c.decodeIfPresent(Bool.self, forKey: .italic) ?? false
        monospaced = try c.decodeIfPresent(Bool.self, forKey: .monospaced) ?? false
        color = try c.decodeIfPresent(WidgetColor.self, forKey: .color)
        underline = try c.decodeIfPresent(Bool.self, forKey: .underline) ?? false
        strikethrough = try c.decodeIfPresent(Bool.self, forKey: .strikethrough) ?? false
        letterSpacing = try c.decodeIfPresent(Double.self, forKey: .letterSpacing)
    }
}

public struct WidgetTextRun: Codable, Sendable, Equatable {
    public var text: String
    public var style: WidgetTextStyle

    public init(text: String, style: WidgetTextStyle = WidgetTextStyle()) {
        self.text = text
        self.style = style
    }
}

public enum WidgetDateMode: String, Codable, Sendable, Equatable {
    case relative, timer, offset, time, date
}

public enum WidgetImageSource: Codable, Sendable, Equatable {
    case asset(path: String)
    case file(path: String)
    case symbol(name: String)

    enum CodingKeys: String, CodingKey {
        case kind, path, name
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "asset":
            self = .asset(path: try c.decode(String.self, forKey: .path))
        case "file":
            self = .file(path: try c.decode(String.self, forKey: .path))
        case "symbol":
            self = .symbol(name: try c.decode(String.self, forKey: .name))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind,
                in: c,
                debugDescription: "Unknown WidgetImageSource kind: \(kind)"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .asset(let path):
            try c.encode("asset", forKey: .kind)
            try c.encode(path, forKey: .path)
        case .file(let path):
            try c.encode("file", forKey: .kind)
            try c.encode(path, forKey: .path)
        case .symbol(let name):
            try c.encode("symbol", forKey: .kind)
            try c.encode(name, forKey: .name)
        }
    }
}

public enum WidgetResizeMode: String, Codable, Sendable, Equatable {
    case contain, cover, stretch, center
}

public enum WidgetAccentedImageMode: String, Codable, Sendable, Equatable {
    case fullColor, accented, desaturated, template
}

/// Platform-neutral Widget Tree IR node.
public indirect enum WidgetNode: Codable, Sendable, Equatable {
    case box(layout: WidgetLayout, style: WidgetBoxStyle, children: [WidgetNode])
    case text(
        runs: [WidgetTextRun],
        align: WidgetTextAlign,
        lineLimit: Int?,
        accentable: Bool,
        layout: WidgetLayout
    )
    case date(
        timestampMs: Int64,
        mode: WidgetDateMode,
        style: WidgetTextStyle,
        accentable: Bool,
        layout: WidgetLayout
    )
    case image(
        source: WidgetImageSource,
        resizeMode: WidgetResizeMode,
        tint: WidgetColor?,
        accentedMode: WidgetAccentedImageMode,
        style: WidgetBoxStyle,
        layout: WidgetLayout
    )
    case progress(
        value: Double,
        trackColor: WidgetColor?,
        indicatorColor: WidgetColor?,
        cornerRadius: Double?,
        layout: WidgetLayout
    )
    case link(href: String, accessibilityLabel: String?, child: WidgetNode)
    case action(
        action: String,
        payloadJson: String?,
        patchJson: String?,
        accessibilityLabel: String?,
        child: WidgetNode
    )

    enum CodingKeys: String, CodingKey {
        case kind
        case layout, style, children
        case runs, align, lineLimit, accentable
        case timestampMs, mode
        case source, resizeMode, tint, accentedMode
        case value, trackColor, indicatorColor, cornerRadius
        case href, accessibilityLabel, child
        case action, payloadJson, patchJson
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "box":
            self = .box(
                layout: try c.decodeIfPresent(WidgetLayout.self, forKey: .layout) ?? WidgetLayout(),
                style: try c.decodeIfPresent(WidgetBoxStyle.self, forKey: .style) ?? WidgetBoxStyle(),
                children: try c.decodeIfPresent([WidgetNode].self, forKey: .children) ?? []
            )
        case "text":
            self = .text(
                runs: try c.decodeIfPresent([WidgetTextRun].self, forKey: .runs) ?? [],
                align: try c.decodeIfPresent(WidgetTextAlign.self, forKey: .align) ?? .leading,
                lineLimit: try c.decodeIfPresent(Int.self, forKey: .lineLimit),
                accentable: try c.decodeIfPresent(Bool.self, forKey: .accentable) ?? false,
                layout: try c.decodeIfPresent(WidgetLayout.self, forKey: .layout) ?? WidgetLayout()
            )
        case "date":
            self = .date(
                timestampMs: try c.decode(Int64.self, forKey: .timestampMs),
                mode: try c.decodeIfPresent(WidgetDateMode.self, forKey: .mode) ?? .relative,
                style: try c.decodeIfPresent(WidgetTextStyle.self, forKey: .style) ?? WidgetTextStyle(),
                accentable: try c.decodeIfPresent(Bool.self, forKey: .accentable) ?? false,
                layout: try c.decodeIfPresent(WidgetLayout.self, forKey: .layout) ?? WidgetLayout()
            )
        case "image":
            self = .image(
                source: try c.decode(WidgetImageSource.self, forKey: .source),
                resizeMode: try c.decodeIfPresent(WidgetResizeMode.self, forKey: .resizeMode) ?? .contain,
                tint: try c.decodeIfPresent(WidgetColor.self, forKey: .tint),
                accentedMode: try c.decodeIfPresent(WidgetAccentedImageMode.self, forKey: .accentedMode) ?? .desaturated,
                style: try c.decodeIfPresent(WidgetBoxStyle.self, forKey: .style) ?? WidgetBoxStyle(),
                layout: try c.decodeIfPresent(WidgetLayout.self, forKey: .layout) ?? WidgetLayout()
            )
        case "progress":
            self = .progress(
                value: try c.decode(Double.self, forKey: .value),
                trackColor: try c.decodeIfPresent(WidgetColor.self, forKey: .trackColor),
                indicatorColor: try c.decodeIfPresent(WidgetColor.self, forKey: .indicatorColor),
                cornerRadius: try c.decodeIfPresent(Double.self, forKey: .cornerRadius),
                layout: try c.decodeIfPresent(WidgetLayout.self, forKey: .layout) ?? WidgetLayout()
            )
        case "link":
            self = .link(
                href: try c.decode(String.self, forKey: .href),
                accessibilityLabel: try c.decodeIfPresent(String.self, forKey: .accessibilityLabel),
                child: try c.decode(WidgetNode.self, forKey: .child)
            )
        case "action":
            self = .action(
                action: try c.decode(String.self, forKey: .action),
                payloadJson: try c.decodeIfPresent(String.self, forKey: .payloadJson),
                patchJson: try c.decodeIfPresent(String.self, forKey: .patchJson),
                accessibilityLabel: try c.decodeIfPresent(String.self, forKey: .accessibilityLabel),
                child: try c.decode(WidgetNode.self, forKey: .child)
            )
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind,
                in: c,
                debugDescription: "Unknown WidgetNode kind: \(kind)"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .box(let layout, let style, let children):
            try c.encode("box", forKey: .kind)
            try c.encode(layout, forKey: .layout)
            try c.encode(style, forKey: .style)
            try c.encode(children, forKey: .children)
        case .text(let runs, let align, let lineLimit, let accentable, let layout):
            try c.encode("text", forKey: .kind)
            try c.encode(runs, forKey: .runs)
            try c.encode(align, forKey: .align)
            try c.encodeIfPresent(lineLimit, forKey: .lineLimit)
            try c.encode(accentable, forKey: .accentable)
            try c.encode(layout, forKey: .layout)
        case .date(let timestampMs, let mode, let style, let accentable, let layout):
            try c.encode("date", forKey: .kind)
            try c.encode(timestampMs, forKey: .timestampMs)
            try c.encode(mode, forKey: .mode)
            try c.encode(style, forKey: .style)
            try c.encode(accentable, forKey: .accentable)
            try c.encode(layout, forKey: .layout)
        case .image(let source, let resizeMode, let tint, let accentedMode, let style, let layout):
            try c.encode("image", forKey: .kind)
            try c.encode(source, forKey: .source)
            try c.encode(resizeMode, forKey: .resizeMode)
            try c.encodeIfPresent(tint, forKey: .tint)
            try c.encode(accentedMode, forKey: .accentedMode)
            try c.encode(style, forKey: .style)
            try c.encode(layout, forKey: .layout)
        case .progress(let value, let trackColor, let indicatorColor, let cornerRadius, let layout):
            try c.encode("progress", forKey: .kind)
            try c.encode(value, forKey: .value)
            try c.encodeIfPresent(trackColor, forKey: .trackColor)
            try c.encodeIfPresent(indicatorColor, forKey: .indicatorColor)
            try c.encodeIfPresent(cornerRadius, forKey: .cornerRadius)
            try c.encode(layout, forKey: .layout)
        case .link(let href, let accessibilityLabel, let child):
            try c.encode("link", forKey: .kind)
            try c.encode(href, forKey: .href)
            try c.encodeIfPresent(accessibilityLabel, forKey: .accessibilityLabel)
            try c.encode(child, forKey: .child)
        case .action(let action, let payloadJson, let patchJson, let accessibilityLabel, let child):
            try c.encode("action", forKey: .kind)
            try c.encode(action, forKey: .action)
            try c.encodeIfPresent(payloadJson, forKey: .payloadJson)
            try c.encodeIfPresent(patchJson, forKey: .patchJson)
            try c.encodeIfPresent(accessibilityLabel, forKey: .accessibilityLabel)
            try c.encode(child, forKey: .child)
        }
    }
}
