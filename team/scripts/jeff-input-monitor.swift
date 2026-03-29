// jeff-input-monitor.swift — System-wide keyboard + mouse activity counter
// Writes keystroke and click counts to /tmp/claude-team-scan/jeff-input.json every 30s.
// Requires Accessibility permissions (System Preferences → Privacy → Accessibility).
// Run as LaunchAgent: com.chorus.jeff-input-monitor

import Foundation
import CoreGraphics

// --- State ---
var keystrokes: Int = 0
var corrections: Int = 0       // backspace-after-keystroke sequences
var lastWasKeystroke: Bool = false  // tracks if previous event was a non-backspace key
var mouseClicks: Int = 0
var scrollEvents: Int = 0
var lastMouseX: Double = 0
var lastMouseY: Double = 0
var mouseMoveDist: Double = 0  // cumulative pixel distance
var lastWriteTime: TimeInterval = Date().timeIntervalSince1970
let outputPath = "/tmp/claude-team-scan/jeff-input.json"
let writeInterval: TimeInterval = 30.0

// --- Callback for CGEventTap ---
func eventCallback(
    proxy: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    userInfo: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    switch type {
    case .keyDown:
        let keyCode = event.getIntegerValueField(.keyboardEventKeycode)
        if keyCode == 51 {  // backspace
            if lastWasKeystroke {
                corrections += 1
            }
            lastWasKeystroke = false
        } else {
            lastWasKeystroke = true
        }
        keystrokes += 1
    case .leftMouseDown, .rightMouseDown, .otherMouseDown:
        mouseClicks += 1
    case .scrollWheel:
        scrollEvents += 1
    case .tapDisabledByTimeout, .tapDisabledByUserInput:
        // Re-enable the tap if it gets disabled
        if let info = userInfo {
            let machPort = Unmanaged<CFMachPort>.fromOpaque(info).takeUnretainedValue()
            CGEvent.tapEnable(tap: machPort, enable: true)
        }
    default:
        break
    }

    let now = Date().timeIntervalSince1970
    if now - lastWriteTime >= writeInterval {
        writeState(now: now)
    }

    return Unmanaged.passRetained(event)
}

// --- Write state to JSON ---
func writeState(now: TimeInterval) {
    let elapsed = now - lastWriteTime
    let keysPerMin = elapsed > 0 ? Double(keystrokes) / (elapsed / 60.0) : 0
    let clicksPerMin = elapsed > 0 ? Double(mouseClicks) / (elapsed / 60.0) : 0
    let scrollsPerMin = elapsed > 0 ? Double(scrollEvents) / (elapsed / 60.0) : 0
    let mouseActive = mouseMoveDist > 50  // >50px movement = mouse in use

    let transpositionRate = keystrokes > 0 ? Double(corrections) / Double(keystrokes) : 0.0

    let json = """
    {"updated":\(Int(now)),"keystrokes_30s":\(keystrokes),"corrections_30s":\(corrections),"transposition_rate":\(String(format: "%.3f", transpositionRate)),"clicks_30s":\(mouseClicks),"scrolls_30s":\(scrollEvents),"mouse_active":\(mouseActive),"keys_per_min":\(String(format: "%.1f", keysPerMin)),"clicks_per_min":\(String(format: "%.1f", clicksPerMin)),"scrolls_per_min":\(String(format: "%.1f", scrollsPerMin))}
    """

    let tmpPath = outputPath + ".tmp"
    do {
        try json.write(toFile: tmpPath, atomically: true, encoding: .utf8)
        try FileManager.default.moveItem(atPath: tmpPath, toPath: outputPath)
    } catch {
        // Atomic write failed — try overwrite
        try? json.write(toFile: outputPath, atomically: false, encoding: .utf8)
    }

    // Reset counters
    keystrokes = 0
    corrections = 0
    lastWasKeystroke = false
    mouseClicks = 0
    scrollEvents = 0
    mouseMoveDist = 0
    lastWriteTime = now
}

// --- Main ---
// Create event tap for key and mouse events
let eventMask: CGEventMask = (1 << CGEventType.keyDown.rawValue)
    | (1 << CGEventType.leftMouseDown.rawValue)
    | (1 << CGEventType.rightMouseDown.rawValue)
    | (1 << CGEventType.otherMouseDown.rawValue)
    | (1 << CGEventType.scrollWheel.rawValue)

guard let eventTap = CGEvent.tapCreate(
    tap: .cgSessionEventTap,
    place: .headInsertEventTap,
    options: .listenOnly,
    eventsOfInterest: eventMask,
    callback: eventCallback,
    userInfo: nil
) else {
    let msg = """
    ERROR: Could not create event tap.
    Grant Accessibility permission: System Preferences → Privacy & Security → Accessibility
    Add: \(CommandLine.arguments[0])
    """
    FileHandle.standardError.write(Data(msg.utf8))
    exit(1)
}

// Wire into run loop
let runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0)
CFRunLoopAddSource(CFRunLoopGetCurrent(), runLoopSource, .commonModes)
CGEvent.tapEnable(tap: eventTap, enable: true)

// Sample mouse position every second via CGEvent (no AppKit needed)
let mouseSampler = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { _ in
    let loc = CGEvent(source: nil)?.location ?? CGPoint.zero
    let dx = loc.x - lastMouseX
    let dy = loc.y - lastMouseY
    let dist = sqrt(dx * dx + dy * dy)
    if lastMouseX != 0 || lastMouseY != 0 {
        mouseMoveDist += dist
    }
    lastMouseX = loc.x
    lastMouseY = loc.y
}
RunLoop.current.add(mouseSampler, forMode: .common)

// Also write state on a timer (in case no events arrive for a while)
let timer = Timer.scheduledTimer(withTimeInterval: writeInterval, repeats: true) { _ in
    writeState(now: Date().timeIntervalSince1970)
}
RunLoop.current.add(timer, forMode: .common)

// Initial state file
writeState(now: Date().timeIntervalSince1970)

// Run forever
CFRunLoopRun()
