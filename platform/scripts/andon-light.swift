import AppKit
import AVFoundation
import Foundation

// MARK: - Priority Model (JS event loop analogy)

enum Priority: String, Comparable {
    case microtask  // Needs Jeff — SWAT, blocked, deploy failed, error burst
    case macrotask  // Brief waiting, workflow step ready
    case render     // Active, executing
    case idle       // Session alive, nothing happening
    case inactive   // No session

    private var sortOrder: Int {
        switch self {
        case .microtask: return 0
        case .macrotask: return 1
        case .render:    return 2
        case .idle:      return 3
        case .inactive:  return 4
        }
    }

    static func < (lhs: Priority, rhs: Priority) -> Bool {
        lhs.sortOrder < rhs.sortOrder
    }
}

// MARK: - State Model

enum RoleState: String {
    case active = "●"       // green light — go
    case struggling = "⬡"   // hexagon — stop sign, needs Jeff
    case waiting = "▲"      // triangle — yield, queued work
    case idle = "◐"         // half — session alive, nothing happening
    case inactive = "○"     // empty — offline
}

struct CardInfo {
    let id: Int
    let title: String
    let status: String
}

struct RoleStatus {
    let name: String
    let initial: String
    let state: RoleState
    let label: String
    let detail: String
    let priority: Priority
    let card: CardInfo?
    let reason: String
    let whyText: String
    let turnRateMin: Double
    let lastBreakAgoMin: Int
}

// MARK: - Jeff Intensity Reader

struct JeffState: Codable {
    let role: String
    let prompts_1h: Int
    let prompts_3h: Int
    let since_last_min: Int
    let break_count_3h: Int
    let intensity: String
    let signal: String
    // Multi-signal fields (optional for backward compat)
    let keys_per_min: Double?
    let clicks_per_min: Double?
    let scrolls_per_min: Double?
    let mouse_active: Bool?
    let idle_duration_min: Int?
    let last_break_time: String?
    let last_break_duration_min: Int?
    let posture: String?
    let tension: String?
    let mood: String?
    let energy: String?
    let posture_fresh: Bool?
    let prompt_type: String?
    let prompt_sentiment: String?
    let behavior: String?
    let composite: String?
}

func readJeffState() -> JeffState? {
    let path = "\(scanDir)/jeff-state.json"
    guard let data = FileManager.default.contents(atPath: path) else { return nil }

    // Stale check: ignore if older than 120s
    if let attrs = try? FileManager.default.attributesOfItem(atPath: path),
       let mtime = attrs[.modificationDate] as? Date,
       -mtime.timeIntervalSinceNow > 120 {
        return nil
    }

    return try? JSONDecoder().decode(JeffState.self, from: data)
}

func colorForIntensity(_ signal: String) -> NSColor {
    switch signal {
    case "green":  return .andonGreen
    case "yellow": return .andonAmber
    case "red":    return .andonRed
    default:       return .andonGray
    }
}

// MARK: - Enrichment JSON Reader

struct DeclaredState: Codable {
    let state: String?
    let detail: String?
}

struct RoleEnrichment: Codable {
    let role: String
    let updated: Int
    let needs_jeff: [String]
    let macrotask: [String]
    let card: EnrichmentCard?
    let gemba: String?  // target role being observed, nil if not in gemba
    let declared: DeclaredState?
    let jsonl_alive: Bool?
    let turn_rate_min: Double?
    let last_break_ago_min: Int?

    struct EnrichmentCard: Codable {
        let id: Int
        let title: String
        let status: String
    }
}

func readEnrichment(role: String) -> RoleEnrichment? {
    let path = "\(scanDir)/\(role)-state.json"
    guard let data = FileManager.default.contents(atPath: path) else { return nil }

    // Stale check: ignore enrichment older than 120s
    if let attrs = try? FileManager.default.attributesOfItem(atPath: path),
       let mtime = attrs[.modificationDate] as? Date,
       -mtime.timeIntervalSinceNow > 120 {
        return nil
    }

    return try? JSONDecoder().decode(RoleEnrichment.self, from: data)
}

// MARK: - Colors

extension NSColor {
    static let andonGreen = NSColor(red: 0.2, green: 0.8, blue: 0.3, alpha: 1.0)
    static let andonRed = NSColor(red: 0.95, green: 0.25, blue: 0.2, alpha: 1.0)
    static let andonAmber = NSColor(red: 0.95, green: 0.65, blue: 0.1, alpha: 1.0)
    static let andonBlue = NSColor(red: 0.3, green: 0.5, blue: 1.0, alpha: 1.0)
    static let andonYellow = NSColor(red: 0.9, green: 0.7, blue: 0.1, alpha: 1.0)
    static let andonGray = NSColor(red: 0.5, green: 0.5, blue: 0.5, alpha: 1.0)
    static let andonBg = NSColor(red: 0.12, green: 0.12, blue: 0.14, alpha: 0.92)
}

func colorForPriority(_ priority: Priority) -> NSColor {
    switch priority {
    case .microtask: return .andonRed      // blocked / needs Jeff
    case .macrotask: return .andonYellow   // queued work waiting
    case .render:    return .andonGreen    // actively working
    case .idle:      return .andonYellow   // session alive, nothing happening
    case .inactive:  return .andonGray     // no session
    }
}

func colorForState(_ state: RoleState) -> NSColor {
    switch state {
    case .active:    return .andonGreen
    case .struggling: return .andonRed
    case .waiting:   return .andonBlue
    case .idle:      return .andonYellow
    case .inactive:  return .andonGray
    }
}

// MARK: - Reason Formatting

func formatReason(enrichment: RoleEnrichment?) -> String {
    guard let e = enrichment else { return "" }
    var parts: [String] = []
    if let first = e.needs_jeff.first {
        parts.append(first.replacingOccurrences(of: "_", with: " "))
    } else if let first = e.macrotask.first {
        parts.append(first.replacingOccurrences(of: "_", with: " "))
    }
    if let target = e.gemba {
        let cap = target.prefix(1).uppercased() + target.dropFirst()
        parts.append("👁 \(cap)")
    }
    return parts.joined(separator: " · ")
}

func formatCardLabel(card: CardInfo?) -> String {
    guard let c = card else { return "" }
    let title = c.title
    let maxLen = 22
    let truncated = title.count > maxLen ? String(title.prefix(maxLen)) + "…" : title
    return "#\(c.id) \(truncated)"
}

// MARK: - State Reader

let scanDir = "/tmp/claude-team-scan"

func readLastPromptTime(role: String) -> Date? {
    let path = "\(scanDir)/\(role)-prompt-times.log"
    guard let data = FileManager.default.contents(atPath: path),
          let content = String(data: data, encoding: .utf8) else {
        return nil
    }
    let lines = content.trimmingCharacters(in: .whitespacesAndNewlines)
        .components(separatedBy: "\n")
    guard let lastLine = lines.last,
          let timestamp = TimeInterval(lastLine) else {
        return nil
    }
    return Date(timeIntervalSince1970: timestamp)
}

func isSessionAlive(role: String) -> Bool {
    let pidPath = "\(scanDir)/\(role).pid"
    guard let data = FileManager.default.contents(atPath: pidPath),
          let content = String(data: data, encoding: .utf8),
          let pid = Int32(content.trimmingCharacters(in: .whitespacesAndNewlines)) else {
        return false
    }
    return kill(pid, 0) == 0
}

func isStruggling(role: String) -> Bool {
    let path = "\(scanDir)/\(role).struggling"
    guard let attrs = try? FileManager.default.attributesOfItem(atPath: path),
          let mtime = attrs[.modificationDate] as? Date else {
        return false
    }
    return -mtime.timeIntervalSinceNow < 90
}

func hasSessionInit(role: String) -> Bool {
    return FileManager.default.fileExists(atPath: "/tmp/claude-session-init/\(role).done")
}

func roleState(role: String) -> RoleStatus {
    let names = ["silas": ("Silas", "S"), "wren": ("Wren", "W"), "kade": ("Kade", "K")]
    let (name, initial) = names[role] ?? (role, String(role.prefix(1)).uppercased())

    let alive = isSessionAlive(role: role)
    let struggling = isStruggling(role: role)
    let enrichment = readEnrichment(role: role)

    // Extract card from enrichment
    let card: CardInfo? = enrichment?.card.map {
        CardInfo(id: $0.id, title: $0.title, status: $0.status)
    }

    // Compute base liveness state + elapsed
    var state: RoleState
    var label: String
    var detail: String

    if let lastPrompt = readLastPromptTime(role: role) {
        let elapsed = -lastPrompt.timeIntervalSinceNow
        detail = formatElapsed(elapsed)

        let hasWipCard = card != nil

        if elapsed <= 120 && struggling {
            state = .struggling; label = "Struggling"
        } else if elapsed <= 120 {
            state = .active; label = "Active"
        } else if alive && hasWipCard {
            // Session alive + WIP card = still working (I/O, deploy, long tool call)
            state = .active; label = "Active"
        } else if alive {
            state = .waiting; label = "Waiting"
        } else if elapsed <= 300 {
            state = .idle; label = "Idle"
        } else {
            state = .inactive; label = "Inactive"
        }
    } else if alive {
        state = .waiting; label = "Waiting"; detail = "for you"
    } else if hasSessionInit(role: role) {
        state = .inactive; label = "Inactive"; detail = "init only"
    } else {
        state = .inactive; label = "Inactive"; detail = "—"
    }

    // Compute priority from enrichment + liveness
    // Active roles stay green (render) — macrotask only escalates waiting/idle
    // Microtask (needs-jeff) always escalates regardless of liveness
    let priority: Priority
    let reason: String

    if let e = enrichment, !e.needs_jeff.isEmpty {
        // Blocked / needs Jeff — always red, even if active
        priority = .microtask
        reason = formatReason(enrichment: enrichment)
    } else if state == .active || state == .struggling {
        // Working — green, regardless of queued macrotasks
        priority = .render
        reason = formatReason(enrichment: enrichment)
    } else if let e = enrichment, !e.macrotask.isEmpty {
        // Not active but has queued work — yellow
        priority = .macrotask
        reason = formatReason(enrichment: enrichment)
    } else if alive {
        priority = .idle
        reason = ""
    } else {
        priority = .inactive
        reason = ""
    }

    // Build "why" text — plain English explanation of current state
    var whyText = ""
    if let e = enrichment {
        // Declared state is most authoritative
        if let d = e.declared, let ds = d.state, !ds.isEmpty {
            switch ds {
            case "building":
                whyText = "building"
            case "blocked":
                whyText = d.detail ?? "blocked"
            case "waiting":
                whyText = "done, waiting"
            case "observing":
                if let g = e.gemba { whyText = "watching \(g)" } else { whyText = "observing" }
            case "idle":
                whyText = "session closed"
            default:
                whyText = ds
            }
        } else if !e.needs_jeff.isEmpty {
            let sig = e.needs_jeff[0].replacingOccurrences(of: "_", with: " ")
            whyText = sig
        } else if !e.macrotask.isEmpty {
            let sig = e.macrotask[0]
            switch sig {
            case "brief_waiting": whyText = "unread brief"
            case "workflow_step": whyText = "workflow ready"
            case "declared_waiting": whyText = "done, waiting"
            default: whyText = sig.replacingOccurrences(of: "_", with: " ")
            }
        } else if state == .active {
            whyText = "executing"
        }
    }
    if whyText.isEmpty {
        switch state {
        case .inactive: whyText = "offline"
        case .idle: whyText = "quiet"
        case .waiting: whyText = "for you"
        default: break
        }
    }

    let turnRate = enrichment?.turn_rate_min ?? 0
    let breakAgo = enrichment?.last_break_ago_min ?? 0

    return RoleStatus(name: name, initial: initial, state: state, label: label,
                      detail: detail, priority: priority, card: card, reason: reason,
                      whyText: whyText, turnRateMin: turnRate, lastBreakAgoMin: breakAgo)
}

func formatElapsed(_ seconds: TimeInterval) -> String {
    let s = Int(seconds)
    if s < 60 { return "\(s)s" }
    let m = s / 60
    let remainder = s % 60
    if m < 60 { return "\(m)m \(remainder)s" }
    let h = m / 60
    return "\(h)h \(m % 60)m"
}

// MARK: - Position Persistence

let positionFile = NSString("~/.chorus/andon-position.json").expandingTildeInPath

func saveGeometry(_ frame: NSRect) {
    let json = "{\"x\":\(frame.origin.x),\"y\":\(frame.origin.y),\"w\":\(frame.width),\"h\":\(frame.height)}"
    try? json.write(toFile: positionFile, atomically: true, encoding: .utf8)
}

func loadGeometry() -> NSRect? {
    guard let data = FileManager.default.contents(atPath: positionFile),
          let json = try? JSONSerialization.jsonObject(with: data) as? [String: Double],
          let x = json["x"], let y = json["y"] else {
        return nil
    }
    let w = json["w"] ?? 160
    let h = json["h"] ?? 600
    return NSRect(x: x, y: y, width: w, height: h)
}

// MARK: - Floating Window

class AndonWindow: NSWindow {
    override var canBecomeKey: Bool { true }

    init() {
        let width: CGFloat = 160
        let height: CGFloat = 600
        let screen = NSScreen.main!.visibleFrame

        let defaultRect = NSRect(
            x: screen.maxX - width - 10,
            y: screen.minY + (screen.height - height) / 2,
            width: width, height: height
        )
        let rect = loadGeometry() ?? defaultRect

        super.init(
            contentRect: rect,
            styleMask: [.titled, .closable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )

        level = .normal
        collectionBehavior = [.managed, .participatesInCycle]
        titleVisibility = .hidden
        styleMask.insert(.miniaturizable)
        titlebarAppearsTransparent = true
        isMovableByWindowBackground = true
        isOpaque = true
        backgroundColor = NSColor(red: 0.12, green: 0.12, blue: 0.14, alpha: 1.0)
        hasShadow = true
        minSize = NSSize(width: 80, height: 300)

        contentView?.wantsLayer = true
        contentView?.layer?.cornerRadius = 12
        contentView?.layer?.masksToBounds = true
        contentView?.autoresizesSubviews = true
    }

    override func mouseUp(with event: NSEvent) {
        super.mouseUp(with: event)
        saveGeometry(frame)
    }
}

// MARK: - Role Cell (vertical: name / dot+state / reason / card / elapsed)

class RoleCellView: NSView {
    let nameLabel = NSTextField(labelWithString: "")
    let dotView = NSTextField(labelWithString: "")
    let stateLabel = NSTextField(labelWithString: "")
    let reasonLabel = NSTextField(labelWithString: "")
    let cardLabel = NSTextField(labelWithString: "")
    let detailLabel = NSTextField(labelWithString: "")

    private var isPulsing = false
    private var pulseTimer: Timer?
    private var pulsePhase: CGFloat = 0

    // Voice-to-session recording state
    private var isRecording = false
    private var audioRecorder: AVAudioRecorder?
    private var recordingPath: String = ""
    private var currentRoleKey: String = ""
    private var savedDotText: String = ""
    private var savedStateText: String = ""

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.backgroundColor = NSColor(red: 0.1, green: 0.1, blue: 0.12, alpha: 1.0).cgColor
        autoresizingMask = [.width]

        nameLabel.alignment = .center
        nameLabel.textColor = .white

        dotView.alignment = .center

        stateLabel.alignment = .center
        stateLabel.textColor = NSColor.white.withAlphaComponent(0.7)

        reasonLabel.alignment = .center
        reasonLabel.textColor = NSColor.white.withAlphaComponent(0.6)
        reasonLabel.lineBreakMode = .byTruncatingTail

        cardLabel.alignment = .center
        cardLabel.textColor = NSColor.white.withAlphaComponent(0.5)
        cardLabel.lineBreakMode = .byTruncatingTail

        detailLabel.alignment = .center
        detailLabel.textColor = NSColor.white.withAlphaComponent(0.4)

        addSubview(nameLabel)
        addSubview(dotView)
        addSubview(stateLabel)
        addSubview(reasonLabel)
        addSubview(cardLabel)
        addSubview(detailLabel)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutCell()
    }

    func layoutCell() {
        let w = bounds.width
        let h = bounds.height
        let scale = min(w / 160.0, h / 190.0)

        let nameSize = max(10, 16 * scale)
        let dotSize = max(20, 48 * scale)
        let stateSize = max(8, 13 * scale)
        let reasonSize = max(7, 10 * scale)
        let cardSize = max(7, 9 * scale)
        let detailSize = max(8, 11 * scale)

        // Stack: name (top) → dot → state → reason → card → detail (bottom)
        let nameH = nameSize * 1.4
        let dotH = dotSize * 1.3
        let stateH = stateSize * 1.4
        let reasonH = reasonSize * 1.3
        let cardH = cardSize * 1.3
        let detailH = detailSize * 1.4
        let totalH = nameH + dotH + stateH + reasonH + cardH + detailH
        let topPad = (h - totalH) / 2

        var y = h - topPad - nameH
        nameLabel.frame = NSRect(x: 0, y: y, width: w, height: nameH)
        nameLabel.font = NSFont.monospacedSystemFont(ofSize: nameSize, weight: .bold)

        y -= dotH
        dotView.frame = NSRect(x: 0, y: y, width: w, height: dotH)
        dotView.font = NSFont.systemFont(ofSize: dotSize)

        y -= stateH
        stateLabel.frame = NSRect(x: 0, y: y, width: w, height: stateH)
        stateLabel.font = NSFont.monospacedSystemFont(ofSize: stateSize, weight: .medium)

        y -= reasonH
        reasonLabel.frame = NSRect(x: 2, y: y, width: w - 4, height: reasonH)
        reasonLabel.font = NSFont.monospacedSystemFont(ofSize: reasonSize, weight: .regular)

        y -= cardH
        cardLabel.frame = NSRect(x: 2, y: y, width: w - 4, height: cardH)
        cardLabel.font = NSFont.monospacedSystemFont(ofSize: cardSize, weight: .regular)

        y -= detailH
        detailLabel.frame = NSRect(x: 0, y: y, width: w, height: detailH)
        detailLabel.font = NSFont.monospacedSystemFont(ofSize: detailSize, weight: .regular)
    }

    func update(status: RoleStatus) {
        nameLabel.stringValue = status.name

        // Don't clobber recording/transcribing visual state
        if isRecording || stateLabel.stringValue == "Transcribing..." || stateLabel.stringValue == "Sent" {
            // Still update card/detail/reason but leave dot + state alone
            cardLabel.stringValue = formatCardLabel(card: status.card)
            reasonLabel.stringValue = status.whyText
            return
        }

        // Override symbol to hexagon (stop sign) when blocked/needs-Jeff
        dotView.stringValue = status.priority == .microtask ? "⬡" : status.state.rawValue
        stateLabel.stringValue = status.label
        detailLabel.stringValue = status.detail
        cardLabel.stringValue = formatCardLabel(card: status.card)

        // Why text — plain English reason for current state
        reasonLabel.stringValue = status.whyText

        // Workload line: turn rate + last break
        if status.turnRateMin > 0 && (status.state == .active || status.state == .struggling) {
            let rateStr = String(format: "%.1fm/turn", status.turnRateMin)
            let breakStr: String
            if status.lastBreakAgoMin <= 0 {
                breakStr = ""
            } else if status.lastBreakAgoMin < 60 {
                breakStr = "brk \(status.lastBreakAgoMin)m"
            } else {
                breakStr = "brk \(status.lastBreakAgoMin / 60)h"
            }
            let parts = [rateStr, breakStr].filter { !$0.isEmpty }
            detailLabel.stringValue = parts.joined(separator: " · ")
        } else {
            detailLabel.stringValue = status.detail
        }

        // Color based on priority (overrides state color when enrichment available)
        let dotColor = colorForPriority(status.priority)
        dotView.textColor = dotColor
        stateLabel.textColor = dotColor.withAlphaComponent(0.8)

        // Reason label color matches priority
        if !status.whyText.isEmpty {
            reasonLabel.textColor = dotColor.withAlphaComponent(0.7)
        } else {
            reasonLabel.textColor = NSColor.white.withAlphaComponent(0.3)
        }

        // Pulse animation for microtask priority only
        if status.priority == .microtask && !isPulsing {
            startPulse()
        } else if status.priority != .microtask && isPulsing {
            stopPulse()
        }
    }

    private func startPulse() {
        isPulsing = true
        pulsePhase = 0
        pulseTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            guard let self = self else { return }
            self.pulsePhase += 0.05
            // Sine wave oscillation: alpha between 0.4 and 1.0
            let alpha = CGFloat(0.7 + 0.3 * sin(self.pulsePhase * .pi * 2))
            self.dotView.alphaValue = alpha
        }
        RunLoop.current.add(pulseTimer!, forMode: .common)
    }

    private func stopPulse() {
        isPulsing = false
        pulseTimer?.invalidate()
        pulseTimer = nil
        dotView.alphaValue = 1.0
    }

    // MARK: - Voice-to-Session

    override func mouseDown(with event: NSEvent) {
        guard !currentRoleKey.isEmpty else {
            super.mouseDown(with: event)
            return
        }

        if isRecording {
            stopVoiceRecording()
        } else {
            startVoiceRecording()
        }
    }

    func setRoleKey(_ key: String) {
        currentRoleKey = key
    }

    private func startVoiceRecording() {
        // Check mic permission first
        let authStatus = AVCaptureDevice.authorizationStatus(for: .audio)
        guard authStatus == .authorized else {
            stateLabel.stringValue = authStatus == .denied ? "Mic denied" : "Mic not ready"
            stateLabel.textColor = NSColor.systemRed
            if authStatus == .notDetermined {
                AVCaptureDevice.requestAccess(for: .audio) { _ in }
            }
            return
        }

        let dir = "/tmp/chorus-listen"
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        let ts = Int(Date().timeIntervalSince1970)
        recordingPath = "\(dir)/andon-\(currentRoleKey)-\(ts).wav"

        let url = URL(fileURLWithPath: recordingPath)
        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatLinearPCM),
            AVSampleRateKey: 16000,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]

        do {
            let recorder = try AVAudioRecorder(url: url, settings: settings)
            recorder.record()
            audioRecorder = recorder
            isRecording = true

            // Visual feedback: red dot + "Recording..."
            savedDotText = dotView.stringValue
            savedStateText = stateLabel.stringValue
            dotView.stringValue = "🔴"
            dotView.textColor = NSColor.systemRed
            stateLabel.stringValue = "Recording..."
            stateLabel.textColor = NSColor.systemRed.withAlphaComponent(0.8)

            NSSound(named: "Tink")?.play()
        } catch {
            stateLabel.stringValue = "Mic error"
            stateLabel.textColor = NSColor.systemRed
        }
    }

    private func stopVoiceRecording() {
        guard let recorder = audioRecorder, recorder.isRecording else { return }
        recorder.stop()
        audioRecorder = nil
        isRecording = false

        // Play stop chime
        NSSound(named: "Pop")?.play()

        // Visual: transcribing state
        dotView.stringValue = "⏳"
        dotView.textColor = NSColor.systemOrange
        stateLabel.stringValue = "Transcribing..."
        stateLabel.textColor = NSColor.systemOrange.withAlphaComponent(0.8)

        // Run voice-to-session.sh in background
        let role = currentRoleKey
        let audioPath = recordingPath
        let savedDot = savedDotText
        let savedState = savedStateText

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let home = ProcessInfo.processInfo.environment["HOME"]!
            let scriptDir = home + "/CascadeProjects/chorus/scripts"
            let vts = Process()
            vts.executableURL = URL(fileURLWithPath: "/bin/bash")
            vts.arguments = ["\(scriptDir)/voice-to-session.sh", role, audioPath]
            // LaunchAgent PATH is minimal — add homebrew so whisper-cli, ffmpeg, brew are found
            var env = ProcessInfo.processInfo.environment
            env["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
            vts.environment = env

            let pipe = Pipe()
            vts.standardOutput = pipe
            vts.standardError = pipe

            do {
                try vts.run()
                vts.waitUntilExit()
                let _ = pipe.fileHandleForReading.readDataToEndOfFile()
                let success = vts.terminationStatus == 0

                DispatchQueue.main.async {
                    guard let self = self else { return }
                    if success {
                        self.dotView.stringValue = "✓"
                        self.dotView.textColor = NSColor.systemGreen
                        self.stateLabel.stringValue = "Sent"
                        self.stateLabel.textColor = NSColor.systemGreen.withAlphaComponent(0.8)
                    } else {
                        self.dotView.stringValue = "✗"
                        self.dotView.textColor = NSColor.systemRed
                        self.stateLabel.stringValue = "Failed"
                        self.stateLabel.textColor = NSColor.systemRed.withAlphaComponent(0.8)
                    }
                    // Restore after 3 seconds
                    DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                        self.dotView.stringValue = savedDot
                        self.stateLabel.stringValue = savedState
                    }
                }
            } catch {
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    self.dotView.stringValue = "✗"
                    self.stateLabel.stringValue = "Script error"
                }
            }
        }
    }
}

// MARK: - Jeff Cell View

class JeffCellView: NSView {
    let nameLabel = NSTextField(labelWithString: "Jeff")
    let dotView = NSTextField(labelWithString: "●")
    let rateLabel = NSTextField(labelWithString: "")
    let whyLabel = NSTextField(labelWithString: "")
    let signalLabel = NSTextField(labelWithString: "")
    let agoLabel = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.backgroundColor = NSColor(red: 0.1, green: 0.1, blue: 0.12, alpha: 1.0).cgColor
        autoresizingMask = [.width]

        nameLabel.alignment = .center
        nameLabel.textColor = .white

        dotView.alignment = .center

        rateLabel.alignment = .center
        rateLabel.textColor = NSColor.white.withAlphaComponent(0.7)

        whyLabel.alignment = .center
        whyLabel.textColor = NSColor.white.withAlphaComponent(0.5)
        whyLabel.lineBreakMode = .byTruncatingTail

        signalLabel.alignment = .center
        signalLabel.textColor = NSColor.white.withAlphaComponent(0.4)
        signalLabel.lineBreakMode = .byTruncatingTail

        agoLabel.alignment = .center
        agoLabel.textColor = NSColor.white.withAlphaComponent(0.4)

        addSubview(nameLabel)
        addSubview(dotView)
        addSubview(rateLabel)
        addSubview(whyLabel)
        addSubview(signalLabel)
        addSubview(agoLabel)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutCell()
    }

    func layoutCell() {
        let w = bounds.width
        let h = bounds.height
        let scale = min(w / 160.0, h / 190.0)

        let nameSize = max(10, 16 * scale)
        let dotSize = max(16, 36 * scale)
        let rateSize = max(8, 13 * scale)
        let whySize = max(7, 9 * scale)
        let signalSize = max(7, 9 * scale)
        let agoSize = max(8, 11 * scale)

        let nameH = nameSize * 1.4
        let dotH = dotSize * 1.3
        let rateH = rateSize * 1.4
        let whyH = whySize * 1.3
        let signalH = signalSize * 1.3
        let agoH = agoSize * 1.4
        let totalH = nameH + dotH + rateH + whyH + signalH + agoH
        let topPad = (h - totalH) / 2

        var y = h - topPad - nameH
        nameLabel.frame = NSRect(x: 0, y: y, width: w, height: nameH)
        nameLabel.font = NSFont.monospacedSystemFont(ofSize: nameSize, weight: .bold)

        y -= dotH
        dotView.frame = NSRect(x: 0, y: y, width: w, height: dotH)
        dotView.font = NSFont.systemFont(ofSize: dotSize)

        y -= rateH
        rateLabel.frame = NSRect(x: 0, y: y, width: w, height: rateH)
        rateLabel.font = NSFont.monospacedSystemFont(ofSize: rateSize, weight: .medium)

        y -= whyH
        whyLabel.frame = NSRect(x: 2, y: y, width: w - 4, height: whyH)
        whyLabel.font = NSFont.monospacedSystemFont(ofSize: whySize, weight: .regular)

        y -= signalH
        signalLabel.frame = NSRect(x: 2, y: y, width: w - 4, height: signalH)
        signalLabel.font = NSFont.monospacedSystemFont(ofSize: signalSize, weight: .regular)

        y -= agoH
        agoLabel.frame = NSRect(x: 0, y: y, width: w, height: agoH)
        agoLabel.font = NSFont.monospacedSystemFont(ofSize: agoSize, weight: .regular)
    }

    func update(state: JeffState?) {
        guard let s = state else {
            dotView.textColor = .andonGray
            rateLabel.stringValue = ""
            whyLabel.stringValue = ""
            signalLabel.stringValue = ""
            agoLabel.stringValue = "away"
            return
        }

        let compositeSignal = s.composite ?? s.signal
        let color = colorForIntensity(compositeSignal)
        dotView.textColor = color

        // Show behavior mode if available, else prompts/hr
        if let behavior = s.behavior, behavior != "unknown" {
            rateLabel.stringValue = behavior
        } else {
            rateLabel.stringValue = "\(s.prompts_1h)/hr"
        }
        rateLabel.textColor = color.withAlphaComponent(0.8)

        // Why line — rate × sentiment state label
        let intensityRaw = s.intensity
        switch intensityRaw {
        case "away":
            whyLabel.stringValue = "away \(s.idle_duration_min ?? s.since_last_min)m"
        case "red":
            if s.prompt_sentiment == "negative" {
                whyLabel.stringValue = "strain · \(s.prompts_1h)/hr"
            } else {
                whyLabel.stringValue = "strain · \(s.prompts_1h)/hr"
            }
        case "yellow":
            if s.prompt_sentiment == "negative" {
                whyLabel.stringValue = "stuck · \(s.prompts_1h)/hr"
            } else if s.tension == "high" {
                whyLabel.stringValue = "tense · \(s.prompts_1h)/hr"
            } else {
                whyLabel.stringValue = "warming · \(s.prompts_1h)/hr"
            }
        case "green":
            if s.prompts_1h > 15 {
                whyLabel.stringValue = "flow · \(s.prompts_1h)/hr"
            } else if s.prompts_1h > 0 {
                whyLabel.stringValue = "reflective"
            } else {
                whyLabel.stringValue = "steady"
            }
        default:
            whyLabel.stringValue = "\(s.prompts_1h)/hr"
        }
        whyLabel.textColor = color.withAlphaComponent(0.6)

        // Signal summary — posture + mood if fresh, else input summary
        if s.posture_fresh == true, let mood = s.mood, let tension = s.tension {
            signalLabel.stringValue = "\(mood) · \(tension)"
        } else {
            let keys = Int(s.keys_per_min ?? 0)
            let mouse = (s.mouse_active ?? false) ? "mouse" : ""
            let parts = ["\(keys)k/m", mouse].filter { !$0.isEmpty }
            signalLabel.stringValue = parts.joined(separator: " · ")
        }

        // Ago line — idle or last break
        if let idle = s.idle_duration_min, idle >= 5 {
            agoLabel.stringValue = "away \(idle)m"
        } else if let breakTime = s.last_break_time, !breakTime.isEmpty {
            agoLabel.stringValue = "brk \(breakTime) \(s.last_break_duration_min ?? 0)m"
        } else if s.since_last_min == 0 {
            agoLabel.stringValue = "now"
        } else if s.since_last_min >= 30 {
            agoLabel.stringValue = "away"
        } else {
            agoLabel.stringValue = "\(s.since_last_min)m ago"
        }
    }
}

// MARK: - Legend View

class LegendView: NSView {
    struct Entry {
        let symbol: String
        let color: NSColor
        let label: String
        let pulse: Bool
    }

    let entries: [Entry] = [
        Entry(symbol: "⬡", color: .andonRed,    label: "Blocked",    pulse: true),
        Entry(symbol: "●", color: .andonGreen,   label: "Working",    pulse: false),
        Entry(symbol: "▲", color: .andonYellow,  label: "Waiting",    pulse: false),
        Entry(symbol: "◐", color: .andonYellow,  label: "Idle",       pulse: false),
        Entry(symbol: "○", color: .andonGray,    label: "Offline",    pulse: false),
    ]

    private var symbolLabels: [NSTextField] = []
    private var textLabels: [NSTextField] = []
    private var divider: NSView?
    private var pulseTimer: Timer?
    private var pulsePhase: CGFloat = 0

    override init(frame: NSRect) {
        super.init(frame: frame)
        autoresizingMask = [.width]

        // Thin divider line at top
        let div = NSView(frame: .zero)
        div.wantsLayer = true
        div.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.15).cgColor
        addSubview(div)
        divider = div

        for entry in entries {
            let sym = NSTextField(labelWithString: entry.symbol)
            sym.textColor = entry.color
            sym.alignment = .right
            addSubview(sym)
            symbolLabels.append(sym)

            let txt = NSTextField(labelWithString: entry.label)
            txt.textColor = NSColor.white.withAlphaComponent(0.5)
            txt.alignment = .left
            addSubview(txt)
            textLabels.append(txt)
        }

        // Pulse the "Needs you" dot
        pulseTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            guard let self = self, !self.symbolLabels.isEmpty else { return }
            self.pulsePhase += 0.05
            let alpha = CGFloat(0.7 + 0.3 * sin(self.pulsePhase * .pi * 2))
            self.symbolLabels[0].alphaValue = alpha
        }
        RunLoop.current.add(pulseTimer!, forMode: .common)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutLegend()
    }

    func layoutLegend() {
        let w = bounds.width
        let h = bounds.height

        divider?.frame = NSRect(x: 8, y: h - 1, width: w - 16, height: 1)

        let rowH = min(14.0, (h - 8) / CGFloat(entries.count))
        let fontSize = max(7, min(10, rowH * 0.75))
        let symW: CGFloat = 20
        let pad: CGFloat = 4

        for (i, _) in entries.enumerated() {
            let y = h - 6 - rowH * CGFloat(i + 1)
            symbolLabels[i].frame = NSRect(x: pad, y: y, width: symW, height: rowH)
            symbolLabels[i].font = NSFont.systemFont(ofSize: fontSize)
            textLabels[i].frame = NSRect(x: pad + symW + 2, y: y, width: w - symW - pad * 2 - 2, height: rowH)
            textLabels[i].font = NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
        }
    }

    static let preferredHeight: CGFloat = 82
}

// MARK: - Content View

class AndonContentView: NSView {
    var cells: [RoleCellView] = []
    let jeffCell = JeffCellView(frame: .zero)
    let legend = LegendView(frame: .zero)
    let jeffDivider = NSView(frame: .zero)

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(jeffCell)
        jeffDivider.wantsLayer = true
        jeffDivider.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.15).cgColor
        addSubview(jeffDivider)
        addSubview(legend)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutCells()
    }

    func layoutCells() {
        let legendH = LegendView.preferredHeight
        let divH: CGFloat = 1
        let totalCells = CGFloat(cells.count + 1)  // roles + Jeff, equal sizing
        let availH = bounds.height - legendH - divH
        let cellH = availH / totalCells

        legend.frame = NSRect(x: 0, y: 0, width: bounds.width, height: legendH)
        legend.layoutLegend()

        // Jeff at top, same size as role cells
        jeffCell.frame = NSRect(x: 0, y: bounds.height - cellH, width: bounds.width, height: cellH)
        jeffCell.layoutCell()

        jeffDivider.frame = NSRect(x: 8, y: bounds.height - cellH - divH, width: bounds.width - 16, height: divH)

        let rolesH = availH - cellH - divH
        let roleCellH = rolesH / CGFloat(cells.count)
        for (i, cell) in cells.enumerated() {
            cell.frame = NSRect(x: 0, y: legendH + rolesH - roleCellH * CGFloat(i + 1),
                                width: bounds.width, height: roleCellH)
            cell.layoutCell()
        }
    }
}

// MARK: - App Delegate

class AndonDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var window: AndonWindow!
    var contentView: AndonContentView!
    var timer: Timer?
    let roles = ["wren", "silas", "kade"]

    func applicationDidFinishLaunching(_ notification: Notification) {
        window = AndonWindow()
        window.delegate = self

        contentView = AndonContentView(frame: window.contentView!.bounds)
        contentView.autoresizingMask = [.width, .height]
        window.contentView!.addSubview(contentView)

        for role in roles {
            let cell = RoleCellView(frame: .zero)
            cell.setRoleKey(role)
            contentView.addSubview(cell)
            contentView.cells.append(cell)
        }
        contentView.layoutCells()

        updateStatus()
        window.orderFrontRegardless()

        timer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            self?.updateStatus()
        }
        RunLoop.current.add(timer!, forMode: .common)

        // Request mic permission at launch so the popup doesn't interrupt recording
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            if !granted {
                DispatchQueue.main.async {
                    NSLog("Andon: Microphone permission denied")
                }
            }
        }
    }

    func windowDidResize(_ notification: Notification) {
        saveGeometry(window.frame)
    }

    func windowDidMove(_ notification: Notification) {
        saveGeometry(window.frame)
    }

    func updateStatus() {
        let statuses = roles.map { roleState(role: $0) }
        for (i, status) in statuses.enumerated() {
            contentView.cells[i].update(status: status)
        }
        contentView.jeffCell.update(state: readJeffState())
    }
}

// MARK: - Main

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = AndonDelegate()
app.delegate = delegate
app.run()
