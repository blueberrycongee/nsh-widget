import Foundation
import Security
import WidgetKit

/// Writes `string` as NUL-terminated UTF-8 into `outBuf` (`outLen` capacity).
/// Returns byte count excluding NUL, or `0` if empty or insufficient capacity.
private func writeCString(_ string: String, to outBuf: UnsafeMutablePointer<CChar>?, capacity outLen: Int) -> Int {
    guard !string.isEmpty else { return 0 }
    let utf8 = Array(string.utf8)
    guard let outBuf, outLen > utf8.count else {
        return utf8.count
    }
    for i in 0..<utf8.count {
        outBuf[i] = CChar(bitPattern: utf8[i])
    }
    outBuf[utf8.count] = 0
    return utf8.count
}

/// Reloads timelines for one widget kind via `WidgetCenter.shared`.
@_cdecl("nsh_widget_reload_kind")
public func nsh_widget_reload_kind(_ kind: UnsafePointer<CChar>?) {
    guard let kind else { return }
    let id = String(cString: kind)
    guard !id.isEmpty else { return }
    WidgetCenter.shared.reloadTimelines(ofKind: id)
}

/// Reloads all timelines for the current app's WidgetKit extensions.
@_cdecl("nsh_widget_reload_all")
public func nsh_widget_reload_all() {
    WidgetCenter.shared.reloadAllTimelines()
}

/// Writes the shared App Group container directory path as UTF-8 (NUL-terminated)
/// into `outBuf` (capacity `outLen`). Returns the path length in bytes (excluding
/// NUL), or `0` if the container is unavailable.
@_cdecl("nsh_widget_group_container")
public func nsh_widget_group_container(
    _ groupId: UnsafePointer<CChar>?,
    _ outBuf: UnsafeMutablePointer<CChar>?,
    _ outLen: Int
) -> Int {
    guard let groupId else { return 0 }
    let id = String(cString: groupId)
    guard !id.isEmpty,
          let url = FileManager.default.containerURL(
              forSecurityApplicationGroupIdentifier: id
          )
    else {
        return 0
    }
    return writeCString(url.path, to: outBuf, capacity: outLen)
}

/// Resolves the first App Group identifier from the running process's code-signing
/// entitlements (`com.apple.security.application-groups`), falling back to the
/// bundle Info.plist key `NshWidgetAppGroup`.
@_cdecl("nsh_widget_default_app_group")
public func nsh_widget_default_app_group(
    _ outBuf: UnsafeMutablePointer<CChar>?,
    _ outLen: Int
) -> Int {
    var code: SecCode?
    if SecCodeCopySelf([], &code) == errSecSuccess, let code {
        var staticCode: SecStaticCode?
        if SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode {
            var info: CFDictionary?
            let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
            if SecCodeCopySigningInformation(staticCode, flags, &info) == errSecSuccess,
               let dict = info as? [String: Any],
               let entitlements = dict[kSecCodeInfoEntitlementsDict as String] as? [String: Any],
               let groups = entitlements["com.apple.security.application-groups"] as? [String],
               let first = groups.first(where: { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty })
            {
                return writeCString(first.trimmingCharacters(in: .whitespacesAndNewlines), to: outBuf, capacity: outLen)
            }
        }
    }
    if let plistGroup = Bundle.main.object(forInfoDictionaryKey: "NshWidgetAppGroup") as? String {
        let trimmed = plistGroup.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            return writeCString(trimmed, to: outBuf, capacity: outLen)
        }
    }
    return 0
}
