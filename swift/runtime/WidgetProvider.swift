import Foundation
import WidgetKit

/// Timeline entry passed from `NshTimelineProvider` into `NshWidgetRootView`.
@available(macOS 14.0, *)
public struct NshWidgetEntry: TimelineEntry {
    public let date: Date
    public let relevance: TimelineEntryRelevance?
    public let widgetId: String
    public let appGroup: String
    public let families: [String: WidgetFamilyTree]
    public let isPlaceholder: Bool

    public init(
        date: Date,
        relevance: TimelineEntryRelevance? = nil,
        widgetId: String,
        appGroup: String,
        families: [String: WidgetFamilyTree],
        isPlaceholder: Bool = false
    ) {
        self.date = date
        self.relevance = relevance
        self.widgetId = widgetId
        self.appGroup = appGroup
        self.families = families
        self.isPlaceholder = isPlaceholder
    }
}

/// Generic `TimelineProvider` reading pre-evaluated `WidgetSnapshot` IR from
/// the shared App Group container (with bundled placeholder fallback).
@available(macOS 14.0, *)
public struct NshTimelineProvider: TimelineProvider {
    public let widgetId: String
    public let appGroup: String

    public init(widgetId: String, appGroup: String) {
        self.widgetId = widgetId
        self.appGroup = appGroup
    }

    public func placeholder(in context: Context) -> NshWidgetEntry {
        let store = NshWidgetStore(appGroup: appGroup)
        if let snapshot = store.loadBundledDefaultSnapshot(widgetId: widgetId)
            ?? store.loadSnapshot(widgetId: widgetId),
           let first = snapshot.entries.first
        {
            return NshWidgetEntry(
                date: Date(),
                relevance: nil,
                widgetId: widgetId,
                appGroup: appGroup,
                families: first.families,
                isPlaceholder: true
            )
        }
        return emptyEntry(isPlaceholder: true)
    }

    public func getSnapshot(
        in context: Context,
        completion: @escaping (NshWidgetEntry) -> Void
    ) {
        let store = NshWidgetStore(appGroup: appGroup)
        let snapshot = context.isPreview
            ? (store.loadSnapshot(widgetId: widgetId) ?? store.loadBundledDefaultSnapshot(widgetId: widgetId))
            : store.loadSnapshot(widgetId: widgetId)
        if let first = snapshot?.entries.first {
            let entry = NshWidgetEntry(
                date: Date(timeIntervalSince1970: Double(first.dateMs) / 1000.0),
                relevance: first.relevance.map { TimelineEntryRelevance(score: Float($0)) },
                widgetId: widgetId,
                appGroup: appGroup,
                families: first.families,
                isPlaceholder: false
            )
            completion(entry)
        } else {
            completion(emptyEntry(isPlaceholder: context.isPreview))
        }
    }

    public func getTimeline(
        in context: Context,
        completion: @escaping (Timeline<NshWidgetEntry>) -> Void
    ) {
        let store = NshWidgetStore(appGroup: appGroup)
        guard let snapshot = store.loadSnapshot(widgetId: widgetId),
              !snapshot.entries.isEmpty
        else {
            completion(Timeline(entries: [emptyEntry(isPlaceholder: true)], policy: .atEnd))
            return
        }

        let entries: [NshWidgetEntry] = snapshot.entries.map { item in
            NshWidgetEntry(
                date: Date(timeIntervalSince1970: Double(item.dateMs) / 1000.0),
                relevance: item.relevance.map { TimelineEntryRelevance(score: Float($0)) },
                widgetId: widgetId,
                appGroup: appGroup,
                families: item.families,
                isPlaceholder: false
            )
        }

        let policy: TimelineReloadPolicy
        if snapshot.reloadNever {
            policy = .never
        } else if let afterMs = snapshot.reloadAfterMs, afterMs > 0 {
            let base = Date(timeIntervalSince1970: Double(snapshot.generatedAtMs) / 1000.0)
            policy = .after(base.addingTimeInterval(Double(afterMs) / 1000.0))
        } else {
            policy = .atEnd
        }
        completion(Timeline(entries: entries, policy: policy))
    }

    private func emptyEntry(isPlaceholder: Bool) -> NshWidgetEntry {
        NshWidgetEntry(
            date: Date(),
            relevance: nil,
            widgetId: widgetId,
            appGroup: appGroup,
            families: [:],
            isPlaceholder: isPlaceholder
        )
    }
}
