import CoreGraphics
import Foundation

// List the windows of one application: id, layer, on-screen flag, bounds,
// title.   usage: swift winlist.swift <owner name part>
let wanted = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "Word"
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID)
    as! [[String: Any]]
for w in list {
    let owner = w["kCGWindowOwnerName"] as? String ?? ""
    if !owner.contains(wanted) { continue }
    let bounds = w["kCGWindowBounds"] as? [String: Any] ?? [:]
    print(
        w["kCGWindowNumber"] ?? "-", "layer", w["kCGWindowLayer"] ?? "-",
        "onscreen", w["kCGWindowIsOnscreen"] ?? 0,
        "w", bounds["Width"] ?? "-", "h", bounds["Height"] ?? "-",
        "x", bounds["X"] ?? "-", "y", bounds["Y"] ?? "-",
        "title:", w["kCGWindowName"] ?? ""
    )
}
