import CoreGraphics
import Foundation

private let minimumWidth = 720
private let minimumHeight = 520

guard CommandLine.arguments.count == 3,
      let processId = pid_t(CommandLine.arguments[1]),
      let timeoutSeconds = Double(CommandLine.arguments[2]),
      timeoutSeconds > 0 else {
    FileHandle.standardError.write(Data("usage: find-a3-window.swift <pid> <timeout-seconds>\n".utf8))
    exit(2)
}

let deadline = Date().addingTimeInterval(timeoutSeconds)

while Date() < deadline {
    let windowList = CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements],
        kCGNullWindowID
    ) as? [[CFString: Any]] ?? []

    for window in windowList {
        guard let owner = window[kCGWindowOwnerPID] as? NSNumber,
              owner.int32Value == processId,
              let layer = window[kCGWindowLayer] as? NSNumber,
              layer.intValue == 0,
              let number = window[kCGWindowNumber] as? NSNumber,
              let boundsDictionary = window[kCGWindowBounds] as? CFDictionary,
              let bounds = CGRect(dictionaryRepresentation: boundsDictionary),
              Int(bounds.width) >= minimumWidth,
              Int(bounds.height) >= minimumHeight else {
            continue
        }

        let title = (window[kCGWindowName] as? String) ?? "A^3"
        print("\(number.uint32Value)|\(Int(bounds.width))|\(Int(bounds.height))|\(title)")
        exit(0)
    }

    Thread.sleep(forTimeInterval: 0.2)
}

FileHandle.standardError.write(
    Data("The A^3 native window did not become visible before the smoke timeout.\n".utf8)
)
exit(1)
