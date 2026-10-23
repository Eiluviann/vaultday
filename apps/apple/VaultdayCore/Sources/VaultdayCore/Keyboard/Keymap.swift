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
	case open
	case toggleSidebar

	/// The action named `name` in Keymap.yaml.
	init?(name: String) {
		switch name {
		case "pane.sidebar": self = .focusPane(.sidebar)
		case "pane.today": self = .focusPane(.today)
		case "pane.inbox": self = .focusPane(.inbox)
		case "pane.side-panel": self = .focusPane(.sidePanel)
		case "move.down": self = .moveDown
		case "move.up": self = .moveUp
		case "move.left": self = .moveLeft
		case "move.right": self = .moveRight
		case "move.first": self = .moveFirst
		case "move.last": self = .moveLast
		case "open": self = .open
		case "sidebar.toggle": self = .toggleSidebar
		default: return nil
		}
	}
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
		self.modifiers = modifiers
	}

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

	static let namedKeys: Set<String> = ["return", "escape", "tab", "space", "up", "down", "left", "right"]

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
/// `version: 1`, and a `bindings:` list of flat `key: value` maps.
public struct Keymap: Sendable {
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
			entries[entries.count - 1].fields[key] = value
		}
		guard version == "1" else { return .failure(KeymapError(line: 1, reason: "expected `version: 1`")) }
		return bindings(from: entries).map(Keymap.init)
	}

	/// The bindings of one scheme as sequences.
	public func sequences(for scheme: KeyScheme) -> [([KeyStroke], KeyAction)] {
		bindings.compactMap { binding in
			guard let sequence = binding.sequences[scheme], !sequence.isEmpty else { return nil }
			return (sequence, binding.action)
		}
	}

	private static func bindings(from entries: [(line: Int, fields: [String: String])]) -> Result<[KeyBinding], KeymapError> {
		var bindings: [KeyBinding] = []
		var seen: [KeyScheme: Set<[KeyStroke]>] = [:]
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
				guard !sequence.isEmpty else { continue }
				guard seen[scheme, default: []].insert(sequence).inserted else {
					return .failure(KeymapError(line: entry.line, reason: "`\(notation)` is bound twice under \(scheme.rawValue)"))
				}
				sequences[scheme] = sequence
			}
			bindings.append(KeyBinding(action: action, sequences: sequences))
		}
		return .success(bindings)
	}

	private static func stripComment(_ line: String) -> String {
		var isQuoted = false
		for (offset, character) in line.enumerated() {
			if character == "\"" { isQuoted.toggle() }
			if character == "#", !isQuoted {
				return String(line.prefix(offset))
			}
		}
		return line
	}

	private static func field(_ text: String) -> (String, String)? {
		guard let colon = text.firstIndex(of: ":") else { return nil }
		let key = text[..<colon].trimmingCharacters(in: .whitespaces)
		var value = text[text.index(after: colon)...].trimmingCharacters(in: .whitespaces)
		if value.count >= 2, value.hasPrefix("\""), value.hasSuffix("\"") {
			value = String(value.dropFirst().dropLast())
		}
		guard !key.isEmpty else { return nil }
		return (key, value)
	}
}
