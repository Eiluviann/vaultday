import Foundation

/// The two keyboard schemes.
public enum KeyScheme: String, CaseIterable, Sendable {
	case vim
	case standard

	public init(_ setting: KeyboardSchemeSetting) {
		self = setting == .vimMotions ? .vim : .standard
	}
}

/// The four panes of the Home window, left to right.
public enum Pane: Int, CaseIterable, Sendable, Codable {
	case sidebar
	case today
	case inbox
	case sidePanel
	/// The story screen's panel: the Story block and its tabs (PR 15).
	case storyPanel

	/// The panes of the Home window, in the order `h` and `l` step through.
	public static let homeOrder: [Pane] = [.sidebar, .today, .inbox, .sidePanel]
	/// The story screen's panes: the note (held by the main area's pane) and its panel.
	public static let storyOrder: [Pane] = [.today, .storyPanel]

	/// The pane named `name` in Keymap.yaml's `pane:` field.
	init?(keymapName name: String) {
		switch name {
		case "sidebar": self = .sidebar
		case "today": self = .today
		case "inbox": self = .inbox
		case "side-panel": self = .sidePanel
		case "story-panel": self = .storyPanel
		default: return nil
		}
	}
}

/// What a binding does.
public enum KeyAction: Hashable, Sendable {
	case focusPane(Pane)
	case moveDown
	case moveUp
	case moveLeft
	case moveRight
	case moveFirst
	case moveLast
	/// `^d` and `^u`: half a pane down or up.
	case moveHalfPageDown
	case moveHalfPageUp
	case open
	case toggleSidebar
	/// ⌘0: every workspace together.
	case showAllWorkspaces
	/// ⌘1 to ⌘9: the workspace at that place in the sidebar, counted from 1.
	case showWorkspace(Int)
	/// `gt` and `gT`: the next and the previous workspace in the sidebar's order.
	case nextWorkspace
	case previousWorkspace
	/// ⌘K, or `:`: the command palette.
	case openPalette
	/// `i`: the quick-add field.
	case addTask
	/// `o` and `O`: a new task after or before the selection.
	case addTaskAfter
	case addTaskBefore
	/// Ticks the selected task done.
	case completeTask
	case moveTaskUp
	case moveTaskDown
	/// `r`: re-plan from now.
	case replan
	/// `=`: auto-schedule the day.
	case autoSchedule
	/// Pins the selected task's box to the private calendar, or unpins it.
	case togglePin
	/// Starts the timer on the selected task, or pauses and resumes it.
	case toggleTimer
	case previousDay
	case nextDay
	case showToday
	/// Esc: closes the toast, the palette or the confirmation.
	case dismiss
	/// `yy`: the selected task into the register, its id onto the pasteboard.
	case yank
	/// `dd`: the selected task off the day into the backlog, and into the register.
	case cut
	/// `p` and `P`: the register's task scheduled on the day shown, after or before the selection.
	case pasteAfter
	case pasteBefore
	/// `u`, `^r`: undo and redo the last change.
	case undo
	case redo
	/// `.`: the last change again, on the selection.
	case repeatLast
	/// `cc`, `ce`, `cd`, `cs`: change the text, estimate, due day or scheduled day.
	case changeText
	case changeEstimate
	case changeDue
	case changeScheduled
	/// `>>` and `<<`: push the task to tomorrow, pull it to today.
	case pushToTomorrow
	case pullToToday
	/// `\`: the selected item's more-actions menu.
	case openItemMenu
	/// ⌘L: the Day log's append field.
	case appendDayLog
	/// `]` and `[`: the next and the previous side panel tab.
	case nextSidePanelTab
	case previousSidePanelTab
	/// `gb`: the Browse table in place of the Today column.
	case showBrowse
	/// `gl`: the side panel's Day log tab.
	case showDayLog
	/// `/`: the Browse query line.
	case focusBrowseQuery
	/// `za`: folds or opens the story group under the selection.
	case toggleFold
	/// `}` and `{`: the next and the previous story group.
	case nextGroup
	case previousGroup
	/// `n` and `N`: the next and the previous row of the query's matches.
	case nextMatch
	case previousMatch
	/// `K`, or a force click: the peek popover of the selected item.
	case peek
	/// `gx`: the selected item's link (its note's `obsidian://` link).
	case openLink
	/// `⌘⇧C`: the selected item in Claude Code, as the preview's button does.
	case openInClaudeCode
	/// A side panel tab by its place on the rail, counted from 1.
	case showSidePanelTab(Int)
	/// The selected signal's outcomes, on the Inbox pane: `1` + Task, `2`
	/// Wait for, `3` Attach, `4` Dismiss, `>>` Snooze, and the menu's Reply
	/// in the mail app and Not a signal.
	case signalTask
	case signalWaitFor
	case signalAttach
	case signalDismiss
	case signalSnooze
	case signalReply
	case signalNotASignal
	/// `]` and `[` while the story screen's panel holds focus.
	case nextStoryPanelTab
	case previousStoryPanelTab
	/// A story panel tab by its place on the rail, counted from 1.
	case showStoryPanelTab(Int)
	/// ⌥⌘T: shows the mini tracker, or toggles its expanded form.
	case toggleMiniTracker

	/// The action named `name` in Keymap.yaml.
	init?(name: String) {
		if let place = Self.place(in: name, after: "workspace.") {
			self = .showWorkspace(place)
			return
		}
		if let place = Self.place(in: name, after: "side-panel.tab.") {
			self = .showSidePanelTab(place)
			return
		}
		if let place = Self.place(in: name, after: "story-panel.tab.") {
			self = .showStoryPanelTab(place)
			return
		}
		guard let action = Self.named[name] else { return nil }
		self = action
	}

	/// Whether this acts on the selected signal rather than a task.
	public var isSignalAction: Bool {
		[.signalTask, .signalWaitFor, .signalAttach, .signalDismiss, .signalSnooze, .signalReply, .signalNotASignal].contains(self)
	}

	/// Whether this is one of the four directional moves bare `h` `j` `k` `l` are kept for.
	public var isDirectionalMove: Bool {
		[.moveLeft, .moveDown, .moveUp, .moveRight].contains(self)
	}

	private static func place(in name: String, after prefix: String) -> Int? {
		guard name.hasPrefix(prefix), let place = Int(name.dropFirst(prefix.count)), (1 ... 9).contains(place) else { return nil }
		return place
	}

	private static let named: [String: KeyAction] = [
		"pane.sidebar": .focusPane(.sidebar),
		"pane.today": .focusPane(.today),
		"pane.inbox": .focusPane(.inbox),
		"pane.side-panel": .focusPane(.sidePanel),
		"move.down": .moveDown,
		"move.up": .moveUp,
		"move.left": .moveLeft,
		"move.right": .moveRight,
		"move.first": .moveFirst,
		"move.last": .moveLast,
		"move.half-page-down": .moveHalfPageDown,
		"move.half-page-up": .moveHalfPageUp,
		"open": .open,
		"sidebar.toggle": .toggleSidebar,
		"workspace.all": .showAllWorkspaces,
		"workspace.next": .nextWorkspace,
		"workspace.previous": .previousWorkspace,
		"palette.open": .openPalette,
		"task.add": .addTask,
		"task.add-after": .addTaskAfter,
		"task.add-before": .addTaskBefore,
		"task.done": .completeTask,
		"task.move-up": .moveTaskUp,
		"task.move-down": .moveTaskDown,
		"plan.replan": .replan,
		"plan.auto-schedule": .autoSchedule,
		"task.pin": .togglePin,
		"timer.toggle": .toggleTimer,
		"day.previous": .previousDay,
		"day.next": .nextDay,
		"day.today": .showToday,
		"dismiss": .dismiss,
		"register.yank": .yank,
		"register.cut": .cut,
		"register.paste-after": .pasteAfter,
		"register.paste-before": .pasteBefore,
		"edit.undo": .undo,
		"edit.redo": .redo,
		"edit.repeat": .repeatLast,
		"change.text": .changeText,
		"change.estimate": .changeEstimate,
		"change.due": .changeDue,
		"change.scheduled": .changeScheduled,
		"task.push-tomorrow": .pushToTomorrow,
		"task.pull-today": .pullToToday,
		"item.menu": .openItemMenu,
		"daylog.append": .appendDayLog,
		"side-panel.next": .nextSidePanelTab,
		"side-panel.previous": .previousSidePanelTab,
		"story-panel.next": .nextStoryPanelTab,
		"story-panel.previous": .previousStoryPanelTab,
		"browse.open": .showBrowse,
		"daylog.open": .showDayLog,
		"browse.query": .focusBrowseQuery,
		"browse.fold": .toggleFold,
		"browse.next-group": .nextGroup,
		"browse.previous-group": .previousGroup,
		"browse.next-match": .nextMatch,
		"browse.previous-match": .previousMatch,
		"peek": .peek,
		"item.open-link": .openLink,
		"item.claude-code": .openInClaudeCode,
		"tracker.toggle": .toggleMiniTracker,
		"signal.task": .signalTask,
		"signal.wait-for": .signalWaitFor,
		"signal.attach": .signalAttach,
		"signal.dismiss": .signalDismiss,
		"signal.snooze": .signalSnooze,
		"signal.reply": .signalReply,
		"signal.not-a-signal": .signalNotASignal,
	]
}

/// Modifier keys of a stroke.
public struct KeyModifiers: OptionSet, Hashable, Sendable {
	public let rawValue: Int
	public init(rawValue: Int) { self.rawValue = rawValue }

	public static let shift = KeyModifiers(rawValue: 1 << 0)
	public static let control = KeyModifiers(rawValue: 1 << 1)
	public static let option = KeyModifiers(rawValue: 1 << 2)
	public static let command = KeyModifiers(rawValue: 1 << 3)
}

/// One key press: a key name (`g`, `return`, `down`) and its modifiers.
public struct KeyStroke: Hashable, Sendable, CustomStringConvertible {
	public let key: String
	public let modifiers: KeyModifiers

	public init(key: String, modifiers: KeyModifiers = []) {
		self.key = key.lowercased()
		// A symbol typed with Shift (`>`, `:`, `}`) always carries it, whether
		// the keymap writes `shift+>` or `>`, so both match the key press.
		self.modifiers = Self.shiftedSymbols.contains(key) ? modifiers.union(.shift) : modifiers
	}

	/// The symbols a US layout types with Shift.
	static let shiftedSymbols: Set<String> = ["~", "!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+", "{", "}", "|", ":", "\"", "<", ">", "?"]

	/// A key without any modifier: `h`, not `H` or `⌥h`.
	public var isBare: Bool { modifiers.isEmpty }

	/// Parses `ctrl+cmd+s`, `shift+g`, `return`.
	public init?(notation: String) {
		let parts = notation.split(separator: "+").map { $0.trimmingCharacters(in: .whitespaces).lowercased() }
		guard let key = parts.last, !key.isEmpty else { return nil }
		var modifiers: KeyModifiers = []
		for part in parts.dropLast() {
			switch part {
			case "shift": modifiers.insert(.shift)
			case "ctrl": modifiers.insert(.control)
			case "opt", "alt": modifiers.insert(.option)
			case "cmd": modifiers.insert(.command)
			default: return nil
			}
		}
		guard key.count == 1 || Self.namedKeys.contains(key) else { return nil }
		self.init(key: key, modifiers: modifiers)
	}

	static let namedKeys: Set<String> = ["return", "escape", "tab", "space", "up", "down", "left", "right", "pageup", "pagedown", "delete"]

	/// The stroke as a key cap shows it: `⌘K`, `⌥↑`, `Space`, `x`.
	public var capText: String {
		// Vim's own spelling for a shifted letter or symbol: `G`, `P`, `>`, `:`.
		if modifiers == .shift, key.count == 1 {
			return key.uppercased()
		}
		var parts: [String] = []
		if modifiers.contains(.control) { parts.append("⌃") }
		if modifiers.contains(.option) { parts.append("⌥") }
		if modifiers.contains(.shift) { parts.append("⇧") }
		if modifiers.contains(.command) { parts.append("⌘") }
		let name: String = switch key {
		case "return": "⏎"
		case "escape": "Esc"
		case "tab": "⇥"
		case "space": "Space"
		case "up": "↑"
		case "down": "↓"
		case "left": "←"
		case "right": "→"
		case "pageup": "⇞"
		case "pagedown": "⇟"
		case "delete": "⌫"
		default: modifiers.isEmpty ? key : key.uppercased()
		}
		let shown = Self.shiftedSymbols.contains(key) ? parts.filter { $0 != "⇧" } : parts
		return shown.joined() + name
	}

	public var description: String {
		var parts: [String] = []
		if modifiers.contains(.control) { parts.append("⌃") }
		if modifiers.contains(.option) { parts.append("⌥") }
		if modifiers.contains(.shift) { parts.append("⇧") }
		if modifiers.contains(.command) { parts.append("⌘") }
		return parts.joined() + key
	}
}

/// One entry of the key table.
public struct KeyBinding: Hashable, Sendable {
	public let action: KeyAction
	public let sequences: [KeyScheme: [KeyStroke]]
	/// What the action does, as the palette and the key table print it.
	public let label: String?
	/// The palette section the action is listed under.
	public let group: String
	/// The footer's short name for the action, when the footer shows it.
	public let tip: String?
	/// Whether the action is also a row of the selected item's more-actions
	/// menu, so a pending stroke that completes it opens the menu.
	public let isInItemMenu: Bool
	/// The pane whose focus the keys need (bare `1` to `7` on the side
	/// panel's rail); `nil` when they act wherever focus is.
	public let pane: Pane?

	public init(
		action: KeyAction,
		sequences: [KeyScheme: [KeyStroke]],
		label: String? = nil,
		group: String = Keymap.defaultGroup,
		tip: String? = nil,
		isInItemMenu: Bool = false,
		pane: Pane? = nil,
	) {
		self.action = action
		self.sequences = sequences
		self.label = label
		self.group = group
		self.tip = tip
		self.isInItemMenu = isInItemMenu
		self.pane = pane
	}
}

/// One action of the key table: its label and every key sequence that runs
/// it under one scheme, in file order.
public struct KeyTableRow: Hashable, Sendable {
	public let action: KeyAction
	public let label: String
	public let group: String
	public let sequences: [[KeyStroke]]
	public let isInItemMenu: Bool

	/// The caps of each sequence: `[["g", "1"]]`, `[["⌘K"], [":"]]`.
	public var caps: [[String]] { sequences.map { $0.map(\.capText) } }
}

/// Why Keymap.yaml was refused; the line is 1-based.
public struct KeymapError: Error, Equatable, Sendable, CustomStringConvertible {
	public let line: Int
	public let reason: String

	public init(line: Int, reason: String) {
		self.line = line
		self.reason = reason
	}

	public var description: String { "Keymap.yaml line \(line): \(reason)" }
}

/// The parsed Keymap.yaml. The file uses a small YAML subset: comments,
/// `version: 1`, and a `bindings:` list of flat `key: value` maps with
/// plain, double-quoted or single-quoted values.
///
/// Parsing enforces the design note's rules, so a broken table never loads:
/// bare `h` `j` `k` `l` only ever move (and start no chord), no sequence is
/// bound twice or hides behind a shorter one within a scheme, and every
/// action carries a label for the palette, the footer and the key table.
public struct Keymap: Sendable {
	public static let defaultGroup = "General"
	/// The keys kept for directional movement, by the action each runs.
	public static let directionalKeys: [String: KeyAction] = ["h": .moveLeft, "j": .moveDown, "k": .moveUp, "l": .moveRight]

	public let bindings: [KeyBinding]

	public init(bindings: [KeyBinding]) {
		self.bindings = bindings
	}

	/// Parses the text of Keymap.yaml.
	public static func parse(_ text: String) -> Result<Keymap, KeymapError> {
		var entries: [(line: Int, fields: [String: String])] = []
		var version: String?
		var isInBindings = false
		for (index, rawLine) in text.components(separatedBy: .newlines).enumerated() {
			let number = index + 1
			let line = stripComment(rawLine)
			guard !line.trimmingCharacters(in: .whitespaces).isEmpty else { continue }
			let trimmed = line.trimmingCharacters(in: .whitespaces)
			if !line.hasPrefix(" ") {
				guard let (key, value) = field(trimmed) else { return .failure(KeymapError(line: number, reason: "expected `key: value`")) }
				switch key {
				case "version": version = value
				case "bindings" where value.isEmpty: isInBindings = true
				default: return .failure(KeymapError(line: number, reason: "unknown top-level key `\(key)`"))
				}
				continue
			}
			guard isInBindings else { return .failure(KeymapError(line: number, reason: "indented line outside `bindings:`")) }
			var content = trimmed
			if content.hasPrefix("- ") {
				entries.append((number, [:]))
				content = String(content.dropFirst(2))
			}
			guard !entries.isEmpty, let (key, value) = field(content) else {
				return .failure(KeymapError(line: number, reason: "expected `- action: …` or `key: value`"))
			}
			guard knownFields.contains(key) else {
				return .failure(KeymapError(line: number, reason: "unknown field `\(key)`"))
			}
			entries[entries.count - 1].fields[key] = value
		}
		guard version == "1" else { return .failure(KeymapError(line: 1, reason: "expected `version: 1`")) }
		return bindings(from: entries).flatMap(validate).map(Keymap.init(bindings:))
	}

	/// The bindings of one scheme as sequences, each with the pane it needs focused.
	public func sequences(for scheme: KeyScheme) -> [(sequence: [KeyStroke], action: KeyAction, pane: Pane?)] {
		bindings.compactMap { binding in
			guard let sequence = binding.sequences[scheme], !sequence.isEmpty else { return nil }
			return (sequence, binding.action, binding.pane)
		}
	}

	/// The key table of one scheme: one row per action in file order, with
	/// every sequence that runs it. Actions the scheme leaves unbound are
	/// kept, with no sequence, so the palette still lists them.
	public func table(for scheme: KeyScheme) -> [KeyTableRow] {
		var order: [KeyAction] = []
		var byAction: [KeyAction: (label: String?, group: String, sequences: [[KeyStroke]], isInItemMenu: Bool)] = [:]
		for binding in bindings {
			if byAction[binding.action] == nil {
				order.append(binding.action)
				byAction[binding.action] = (binding.label, binding.group, [], binding.isInItemMenu)
			}
			if let sequence = binding.sequences[scheme], !sequence.isEmpty {
				byAction[binding.action]?.sequences.append(sequence)
			}
			if byAction[binding.action]?.label == nil {
				byAction[binding.action]?.label = binding.label
			}
		}
		return order.compactMap { action in
			guard let row = byAction[action] else { return nil }
			return KeyTableRow(action: action, label: row.label ?? "", group: row.group, sequences: row.sequences, isInItemMenu: row.isInItemMenu)
		}
	}

	/// The footer's tips in file order: the short name and the action.
	public var tips: [(name: String, action: KeyAction)] {
		bindings.compactMap { binding in binding.tip.map { ($0, binding.action) } }
	}

	private static let knownFields: Set<String> = ["action", "vim", "standard", "label", "group", "tip", "menu", "pane"]

	private static func bindings(from entries: [(line: Int, fields: [String: String])]) -> Result<[(line: Int, binding: KeyBinding)], KeymapError> {
		var bindings: [(line: Int, binding: KeyBinding)] = []
		for entry in entries {
			guard let name = entry.fields["action"] else { return .failure(KeymapError(line: entry.line, reason: "a binding without `action`")) }
			guard let action = KeyAction(name: name) else { return .failure(KeymapError(line: entry.line, reason: "unknown action `\(name)`")) }
			var sequences: [KeyScheme: [KeyStroke]] = [:]
			for scheme in KeyScheme.allCases {
				let notation = entry.fields[scheme.rawValue] ?? ""
				let strokes = notation.split(separator: " ").map { KeyStroke(notation: String($0)) }
				guard strokes.allSatisfy({ $0 != nil }) else {
					return .failure(KeymapError(line: entry.line, reason: "`\(notation)` is not a key sequence"))
				}
				let sequence = strokes.compactMap(\.self)
				if !sequence.isEmpty { sequences[scheme] = sequence }
			}
			let menu = entry.fields["menu"] ?? "false"
			guard menu == "true" || menu == "false" else {
				return .failure(KeymapError(line: entry.line, reason: "`menu` is `true` or `false`"))
			}
			let label = entry.fields["label"].flatMap { $0.isEmpty ? nil : $0 }
			var pane: Pane?
			if let paneName = entry.fields["pane"], !paneName.isEmpty {
				guard let named = Pane(keymapName: paneName) else {
					return .failure(KeymapError(line: entry.line, reason: "unknown pane `\(paneName)`"))
				}
				pane = named
			}
			let binding = KeyBinding(
				action: action,
				sequences: sequences,
				label: label,
				group: entry.fields["group"].flatMap { $0.isEmpty ? nil : $0 } ?? defaultGroup,
				tip: entry.fields["tip"].flatMap { $0.isEmpty ? nil : $0 },
				isInItemMenu: menu == "true",
				pane: pane,
			)
			bindings.append((entry.line, binding))
		}
		return .success(bindings)
	}

	/// The design note's rules over the parsed bindings.
	private static func validate(_ bindings: [(line: Int, binding: KeyBinding)]) -> Result<[KeyBinding], KeymapError> {
		var labelled: Set<KeyAction> = []
		var firstLine: [KeyAction: Int] = [:]
		for (line, binding) in bindings {
			if binding.label != nil { labelled.insert(binding.action) }
			if firstLine[binding.action] == nil { firstLine[binding.action] = line }
		}
		if let (action, line) = firstLine.sorted(by: { $0.value < $1.value }).first(where: { !labelled.contains($0.key) }) {
			return .failure(KeymapError(line: line, reason: "the action \(action) has no `label`"))
		}
		for scheme in KeyScheme.allCases {
			// Each sequence with the line and pane of every binding of it; two
			// bindings of one sequence clash when they can act at once.
			var seen: [[KeyStroke]: [(line: Int, pane: Pane?)]] = [:]
			for (line, binding) in bindings {
				guard let sequence = binding.sequences[scheme], let first = sequence.first else { continue }
				if first.isBare, let kept = directionalKeys[first.key], sequence.count > 1 || binding.action != kept {
					return .failure(KeymapError(line: line, reason: "bare `\(first.key)` is directional movement only, under \(scheme.rawValue)"))
				}
				if let earlier = seen[sequence]?.first(where: { clashes($0.pane, binding.pane) }) {
					return .failure(KeymapError(line: line, reason: "`\(spelled(sequence))` is bound twice under \(scheme.rawValue) (line \(earlier.line) too)"))
				}
				seen[sequence, default: []].append((line, binding.pane))
			}
			// A sequence that starts a longer one would fire before the longer one could be typed.
			for (sequence, longer) in seen {
				for length in 1 ..< max(sequence.count, 1) {
					let prefix = Array(sequence.prefix(length))
					let hiding = seen[prefix]?.first { shorter in longer.contains { overlaps(shorter.pane, $0.pane) } }
					if let hiding {
						return .failure(KeymapError(line: hiding.line, reason: "`\(spelled(prefix))` hides `\(spelled(sequence))` under \(scheme.rawValue)"))
					}
				}
			}
		}
		return .success(bindings.map(\.binding))
	}

	/// Whether two bindings of one sequence clash: both need no pane, both
	/// the same pane, or one needs none and the other a pane of the Home
	/// window. A binding scoped to a pane outside Home (the story panel)
	/// shadows an unscoped one there, so the two never act at once.
	private static func clashes(_ left: Pane?, _ right: Pane?) -> Bool {
		if left == right { return true }
		guard left == nil || right == nil, let scoped = left ?? right else { return false }
		return Pane.homeOrder.contains(scoped)
	}

	/// Whether bindings needing these panes can both act at one time.
	private static func overlaps(_ left: Pane?, _ right: Pane?) -> Bool {
		left == nil || right == nil || left == right
	}

	private static func spelled(_ sequence: [KeyStroke]) -> String {
		sequence.map(\.description).joined(separator: " ")
	}

	private static func stripComment(_ line: String) -> String {
		var quote: Character?
		for (offset, character) in line.enumerated() {
			if character == "\"" || character == "'" {
				if quote == nil { quote = character } else if quote == character { quote = nil }
			}
			if character == "#", quote == nil {
				return String(line.prefix(offset))
			}
		}
		return line
	}

	private static func field(_ text: String) -> (String, String)? {
		guard let colon = text.firstIndex(of: ":") else { return nil }
		let key = text[..<colon].trimmingCharacters(in: .whitespaces)
		var value = text[text.index(after: colon)...].trimmingCharacters(in: .whitespaces)
		for quote in ["\"", "'"] where value.count >= 2 && value.hasPrefix(quote) && value.hasSuffix(quote) {
			value = String(value.dropFirst().dropLast())
			break
		}
		guard !key.isEmpty else { return nil }
		return (key, value)
	}
}
