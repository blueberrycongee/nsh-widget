import AppIntents
import Foundation
import WidgetKit

/// Generic AppIntent invoked when a user clicks a `<WAction>` button
/// on a macOS 14+ Desktop or Notification Center widget.
@available(macOS 14.0, *)
public struct NshWidgetActionIntent: AppIntent {
    public static var title: LocalizedStringResource = "Run Native Shell Widget Action"
    public static var description = IntentDescription(
        "Dispatches an interactive widget button action to the Native Shell app."
    )
    public static var openAppWhenRun: Bool = false

    @Parameter(title: "Widget ID")
    var widgetId: String

    @Parameter(title: "App Group")
    var appGroup: String

    @Parameter(title: "Action")
    var action: String

    @Parameter(title: "Payload JSON")
    var payloadJson: String?

    @Parameter(title: "Patch JSON")
    var patchJson: String?

    public init() {
        self.widgetId = ""
        self.appGroup = ""
        self.action = ""
        self.payloadJson = nil
        self.patchJson = nil
    }

    public init(
        widgetId: String,
        appGroup: String,
        action: String,
        payloadJson: String? = nil,
        patchJson: String? = nil
    ) {
        self.widgetId = widgetId
        self.appGroup = appGroup
        self.action = action
        self.payloadJson = payloadJson
        self.patchJson = patchJson
    }

    public func perform() async throws -> some IntentResult {
        let store = NshWidgetStore(appGroup: appGroup)
        if let patchJson, !patchJson.isEmpty {
            store.applyOfflinePatch(widgetId: widgetId, patchJson: patchJson)
        }
        store.enqueueAction(widgetId: widgetId, action: action, payloadJson: payloadJson)
        WidgetCenter.shared.reloadTimelines(ofKind: widgetId)
        return .result()
    }
}
