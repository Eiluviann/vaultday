import Foundation
import Observation

/// What one stroke resolved to.
public enum KeyResolution: Equatable, Sendable {
	/// A binding completed.
	case action(KeyAction)
	/// The strokes so far start one or more bindings; the next stroke decides.
	case pending([KeyStroke])
	/// Esc dropped a pending prefix and did nothing else.
	case cancelled
	/// No binding starts with these strokes; the stroke goes on to the system.
	case unbound
}

/// A key that completes the pending strokes, for the which-key highlights.
public struct KeyCompletion: Hashable, Sendable {
	/// The stroke still to type (`1` after `g`, `y` after `y`).
	public let next: KeyStroke
	/// The whole sequence, pending strokes included.
	public let sequence: [KeyStroke]
	public let action: KeyAction
	public let label: String
	/// Whether the action is a row of the selected item's more-actions menu.
	public let isInItemMenu: Bool
}

/// Turns strokes into actions for the current scheme, holding a pending
/// prefix (`g` of `g1`) between strokes. ``pendingText`` is the showcmd.
///
/// A pending prefix never lapses, as in an editor: it waits for the next
/// stroke, and Esc drops it without doing anything else.
@MainActor
@Observable
public final class KeyDispatcher {
	public private(set) var scheme: KeyScheme
	public private(set) var pending: [KeyStroke] = []
	/// The keys that complete ``pending``, with what each runs; empty when nothing is pending.
	public private(set) var completions: [KeyCompletion] = []
	public let keymap: Keymap

	@ObservationIgnored private var bound: [BoundSequence] = []
	@ObservationIgnored private var rows: [KeyTableRow] = []
	/// The bindings each pane reaches on each screen, worked out once per scheme.
	@ObservationIgnored private var reachableCache: [ReachKey: [BoundSequence]] = [:]

	private struct ReachKey: Hashable {
		let pane: Pane?
		let screen: [Pane]
	}

	private static let escape = KeyStroke(key: "escape")

	public init(keymap: Keymap, scheme: KeyScheme) {
		self.keymap = keymap
		self.scheme = scheme
		rebuild()
	}

	/// The pending strokes as the showcmd indicator prints them.
	public var pendingText: String {
		pending.map(\.capText).joined()
	}

	/// Whether a pending `g` can jump to a pane, so every pane shows its number.
	public var showsPaneNumbers: Bool {
		completions.contains { if case .focusPane = $0.action { true } else { false } }
	}

	/// Whether a completion lives in the selected item's menu, which then opens by itself.
	public var opensItemMenu: Bool {
		completions.contains(where: \.isInItemMenu)
	}

	/// The key table of the current scheme, as the palette and the footer render it.
	public var keyTable: [KeyTableRow] { rows }

	/// The footer's tips that the current scheme binds, in file order.
	public var tips: [(name: String, action: KeyAction)] {
		keymap.tips.filter { label(for: $0.action) != nil }
	}

	/// Switches scheme, dropping any pending prefix.
	public func setScheme(_ scheme: KeyScheme) {
		self.scheme = scheme
		clearPending()
		rebuild()
	}

	/// Feeds one stroke typed while `pane` holds focus, with `screen` the
	/// panes on screen. Which bindings answer, strongest first: one scoped
	/// to `pane`, one scoped to no pane, then a fallback scoped to another
	/// pane of `screen` (`[` `]` reach the only tabbed pane). A binding that
	/// needs another pane (bare `1` to `7` need the side panel) stays out of reach.
	public func handle(_ stroke: KeyStroke, in pane: Pane? = nil, screen: [Pane] = Pane.homeOrder) -> KeyResolution {
		if !pending.isEmpty, stroke == Self.escape {
			clearPending()
			return .cancelled
		}
		let sequence = pending + [stroke]
		let reachable = reachableBindings(in: pane, screen: screen)
		if let match = reachable.first(where: { $0.sequence == sequence }) {
			clearPending()
			return .action(match.action)
		}
		let completing = reachable.filter { $0.sequence.count > sequence.count && Array($0.sequence.prefix(sequence.count)) == sequence }
		if !completing.isEmpty {
			setPending(sequence, completing: completing.map(\.sequence))
			return .pending(sequence)
		}
		// A stroke that breaks a pending prefix starts over on its own.
		if !pending.isEmpty {
			clearPending()
			return handle(stroke, in: pane, screen: screen)
		}
		return .unbound
	}

	/// Drops a pending prefix (Esc, focus change).
	public func cancelPending() {
		clearPending()
	}

	/// The keys that run `action` under the current scheme, as a cap shows
	/// them (`⌘K`, `x`, `Space`, `g b`); `nil` when the scheme leaves it unbound.
	public func label(for action: KeyAction) -> String? {
		caps(for: action).first?.joined(separator: " ")
	}

	/// Every sequence that runs `action`, each as its caps.
	public func caps(for action: KeyAction) -> [[String]] {
		rows.first { $0.action == action }?.caps ?? []
	}

	/// What `action` does, from the key table.
	public func title(for action: KeyAction) -> String? {
		rows.first { $0.action == action }?.label
	}

	/// The bindings `pane` reaches, strongest first; a fallback only for keys
	/// that neither `pane` nor the unscoped bindings use.
	private func reachableBindings(in pane: Pane?, screen: [Pane]) -> [BoundSequence] {
		let key = ReachKey(pane: pane, screen: screen)
		if let cached = reachableCache[key] { return cached }
		let scoped = bound.filter { $0.pane != nil && $0.pane == pane }
		let unscoped = bound.filter { $0.pane == nil }
		let claimed = Set((scoped + unscoped).map(\.sequence))
		let fallbacks = bound.filter { binding in
			guard binding.isFallback, let target = binding.pane, target != pane, screen.contains(target) else { return false }
			return !claimed.contains(binding.sequence)
		}
		let reachable = scoped + unscoped + fallbacks
		reachableCache[key] = reachable
		return reachable
	}

	private func setPending(_ sequence: [KeyStroke], completing reachable: [[KeyStroke]]) {
		pending = sequence
		let reachable = Set(reachable)
		completions = rows.flatMap { row in
			row.sequences.compactMap { candidate -> KeyCompletion? in
				guard candidate.count == sequence.count + 1, reachable.contains(candidate), let next = candidate.last else { return nil }
				return KeyCompletion(next: next, sequence: candidate, action: row.action, label: row.label, isInItemMenu: row.isInItemMenu)
			}
		}
	}

	private func clearPending() {
		guard !pending.isEmpty || !completions.isEmpty else { return }
		pending = []
		completions = []
	}

	private func rebuild() {
		rows = keymap.table(for: scheme)
		bound = keymap.sequences(for: scheme)
		reachableCache = [:]
	}
}
