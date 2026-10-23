import Foundation
import Observation

/// What one stroke resolved to.
public enum KeyResolution: Equatable, Sendable {
	/// A binding completed.
	case action(KeyAction)
	/// The strokes so far start one or more bindings; the next stroke decides.
	case pending([KeyStroke])
	/// No binding starts with these strokes; the stroke goes on to the system.
	case unbound
}

/// Turns strokes into actions for the current scheme, holding a pending
/// prefix (`g` of `g1`) between strokes. ``pendingText`` is the showcmd.
@MainActor
@Observable
public final class KeyDispatcher {
	public private(set) var scheme: KeyScheme
	public private(set) var pending: [KeyStroke] = []

	private let keymap: Keymap
	private var table: [[KeyStroke]: KeyAction] = [:]
	private var prefixes: Set<[KeyStroke]> = []

	public init(keymap: Keymap, scheme: KeyScheme) {
		self.keymap = keymap
		self.scheme = scheme
		rebuild()
	}

	/// The pending strokes as the showcmd indicator prints them.
	public var pendingText: String {
		pending.map(\.description).joined()
	}

	/// Switches scheme, dropping any pending prefix.
	public func setScheme(_ scheme: KeyScheme) {
		self.scheme = scheme
		pending = []
		rebuild()
	}

	/// Feeds one stroke.
	public func handle(_ stroke: KeyStroke) -> KeyResolution {
		let sequence = pending + [stroke]
		if let action = table[sequence] {
			pending = []
			return .action(action)
		}
		if prefixes.contains(sequence) {
			pending = sequence
			return .pending(sequence)
		}
		// A stroke that breaks a pending prefix starts over on its own.
		if !pending.isEmpty {
			pending = []
			return handle(stroke)
		}
		return .unbound
	}

	/// Drops a pending prefix (Esc, focus change).
	public func cancelPending() {
		pending = []
	}

	private func rebuild() {
		table = [:]
		prefixes = []
		for (sequence, action) in keymap.sequences(for: scheme) {
			table[sequence] = action
			for length in 1 ..< max(sequence.count, 1) {
				prefixes.insert(Array(sequence.prefix(length)))
			}
		}
	}
}
