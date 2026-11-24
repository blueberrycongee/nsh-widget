import AppKit
import CoreFoundation
import Foundation

/// App Group file store shared between the main Native Shell app and the
/// macOS WidgetKit extension.
public final class NshWidgetStore: Sendable {
    public let appGroup: String

    public init(appGroup: String) {
        self.appGroup = appGroup
    }

    /// Resolves the shared App Group container directory, falling back to
    /// Application Support when running an ad-hoc build without a provisioned
    /// container.
    public func containerURL() -> URL? {
        if let groupURL = FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: appGroup
        ) {
            return groupURL
        }
        if let appSupport = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first {
            return appSupport.appendingPathComponent("nsh-widgets/\(appGroup)", isDirectory: true)
        }
        return nil
    }

    /// Root `widgets/` directory inside the shared container.
    public func widgetsRootURL() -> URL? {
        containerURL()?.appendingPathComponent("widgets", isDirectory: true)
    }

    /// Loads the persisted snapshot for `widgetId` from the App Group container,
    /// falling back to the extension bundle's pre-evaluated `default-snapshots.json`.
    public func loadSnapshot(widgetId: String) -> WidgetSnapshot? {
        let decoder = JSONDecoder()
        if let url = widgetsRootURL()?
            .appendingPathComponent(widgetId, isDirectory: true)
            .appendingPathComponent("snapshot.json"),
           let data = try? Data(contentsOf: url),
           let snapshot = try? decoder.decode(WidgetSnapshot.self, from: data)
        {
            return snapshot
        }
        return loadBundledDefaultSnapshot(widgetId: widgetId)
    }

    /// Loads the compile-time placeholder snapshot bundled inside the `.appex`.
    public func loadBundledDefaultSnapshot(widgetId: String) -> WidgetSnapshot? {
        guard let url = Bundle.main.url(
            forResource: "default-snapshots",
            withExtension: "json",
            subdirectory: "nsh/widgets"
        ),
              let data = try? Data(contentsOf: url),
              let map = try? JSONDecoder().decode([String: WidgetSnapshot].self, from: data)
        else {
            return nil
        }
        return map[widgetId]
    }

    /// Resolves an image referenced by a `WidgetImageSource` from the App Group
    /// container or the extension's bundled resources.
    public func loadImage(source: WidgetImageSource, widgetId: String) -> NSImage? {
        switch source {
        case .symbol:
            return nil
        case .asset(let path):
            let clean = sanitizeRelativePath(path)
            if let groupAsset = widgetsRootURL()?
                .appendingPathComponent(widgetId, isDirectory: true)
                .appendingPathComponent("assets", isDirectory: true)
                .appendingPathComponent(clean),
               let img = NSImage(contentsOf: groupAsset)
            {
                return img
            }
            if let resourceRoot = Bundle.main.resourceURL {
                let bundled = resourceRoot
                    .appendingPathComponent("nsh/widgets/assets", isDirectory: true)
                    .appendingPathComponent(clean)
                if let img = NSImage(contentsOf: bundled) {
                    return img
                }
            }
            return nil
        case .file(let path):
            let clean = sanitizeRelativePath(path)
            if let url = widgetsRootURL()?.appendingPathComponent(clean),
               let img = NSImage(contentsOf: url)
            {
                return img
            }
            return nil
        }
    }

    /// Applies an optional JSON object patch to `widgets/<widgetId>/data.json`
    /// atomically for optimistic offline feedback.
    public func applyOfflinePatch(widgetId: String, patchJson: String) {
        guard let root = widgetsRootURL(),
              let patchData = patchJson.data(using: .utf8),
              let patchObj = try? JSONSerialization.jsonObject(with: patchData) as? [String: Any]
        else {
            return
        }
        let dir = root.appendingPathComponent(widgetId, isDirectory: true)
        let dataURL = dir.appendingPathComponent("data.json")
        var merged: [String: Any] = [:]
        if let existing = try? Data(contentsOf: dataURL),
           let obj = try? JSONSerialization.jsonObject(with: existing) as? [String: Any]
        {
            merged = obj
        }
        for (k, v) in patchObj {
            merged[k] = v
        }
        if let encoded = try? JSONSerialization.data(withJSONObject: merged, options: []) {
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try? encoded.write(to: dataURL, options: .atomic)
        }
    }

    /// Appends a `WidgetActionEvent` JSON line into `widgets/actions.jsonl`
    /// and posts a Darwin notification so a running main app picks it up
    /// immediately without polling.
    public func enqueueAction(widgetId: String, action: String, payloadJson: String?) {
        guard let root = widgetsRootURL() else { return }
        try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let actionsURL = root.appendingPathComponent("actions.jsonl")

        var payloadObject: Any = NSNull()
        if let payloadJson,
           let raw = payloadJson.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: raw)
        {
            payloadObject = parsed
        }
        let timestampMs = Int64((Date().timeIntervalSince1970 * 1000.0).rounded())
        var record: [String: Any] = [
            "widgetId": widgetId,
            "action": action,
            "timestamp": timestampMs,
        ]
        if !(payloadObject is NSNull) {
            record["payload"] = payloadObject
        }
        guard let lineData = try? JSONSerialization.data(withJSONObject: record, options: []),
              var line = String(data: lineData, encoding: .utf8)
        else {
            return
        }
        line.append("\n")
        if let bytes = line.data(using: .utf8) {
            if FileManager.default.fileExists(atPath: actionsURL.path),
               let handle = try? FileHandle(forWritingTo: actionsURL)
            {
                _ = try? handle.seekToEnd()
                try? handle.write(contentsOf: bytes)
                try? handle.close()
            } else {
                try? bytes.write(to: actionsURL, options: .atomic)
            }
        }
        postActionNotification()
    }

    /// Posts the Darwin notification `<appGroup>.action` watched by the main app.
    public func postActionNotification() {
        let name = CFNotificationName("\(appGroup).action" as CFString)
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            name,
            nil,
            nil,
            true
        )
    }

    private func sanitizeRelativePath(_ path: String) -> String {
        let stripped = path.hasPrefix("asset:/") ? String(path.dropFirst(7)) : path
        return stripped
            .split(separator: "/")
            .filter { !$0.isEmpty && $0 != "." && $0 != ".." }
            .joined(separator: "/")
    }
}
