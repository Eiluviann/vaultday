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
/// A pending prefix lapses after ``pendingTimeout`` without a stroke, as
/// vim's `timeoutlen`: the next stroke then starts over on its own. The view
/// calls ``expirePending()`` once the timeout passes so the highlights leave.
@MainActor
@Observable
public final class KeyDispatcher {
	/// How long a pending prefix waits for its next stroke.
	public static let defaultPendingTimeout: Duration = .seconds(1)

	public private(set) var scheme: KeyScheme
	public private(set) var pending: [KeyStroke] = []
	/// The keys that complete ``pending``, with what each runs; empty when nothing is pending.
	public private(set) var completions: [KeyCompletion] = []
	public let pendingTimeout: Duration
	public let keymap: Keymap

	@ObservationIgnored private let now: () -> ContinuousClock.Instant
	@ObservationIgnored private var pendingSince: ContinuousClock.Instant?
	@ObservationIgnored private var bound: [(sequence: [KeyStroke], action: KeyAction, pane: Pane?)] = []
	@ObservationIgnored private var rows: [KeyTableRow] = []

	public init(
		keymap: Keymap,
		scheme: KeyScheme,
		pendingTimeout: Duration = KeyDispatcher.defaultPendingTimeout,
		now: @escaping () -> ContinuousClock.Instant = { ContinuousClock.now },
	) {
		self.keymap = keymap
		self.scheme = scheme
		self.pendingTimeout = pendingTimeout
		self.now = now
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

	/// Feeds one stroke typed while `pane` holds focus: a binding that needs
	/// another pane (bare `1` to `7` need the side panel) stays out of reach,
	/// and one scoped to `pane` wins over an unscoped one of the same keys.
	public func handle(_ stroke: KeyStroke, in pane: Pane? = nil) -> KeyResolution {
		expirePending()
		let sequence = pending + [stroke]
		let reachable = bound.filter { $0.pane == nil || $0.pane == pane }
		// A binding scoped to the focused pane shadows an unscoped one of the
		// same keys; the keymap allows that only outside the Home window.
		let exact = reachable.filter { $0.sequence == sequence }
		if let match = exact.first(where: { $0.pane != nil }) ?? exact.first {
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
			return handle(stroke, in: pane)
		}
		return .unbound
	}

	/// Drops a pending prefix whose timeout has passed; `true` when one was dropped.
	@discardableResult
	public func expirePending() -> Bool {
		guard let pendingSince, now() - pendingSince >= pendingTimeout else { return false }
		clearPending()
		return true
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

	private func setPending(_ sequence: [KeyStroke], completing reachable: [[KeyStroke]]) {
		pending = sequence
		pendingSince = now()
		completions = rows.flatMap { row in
			row.sequences.compactMap { candidate -> KeyCompletion? in
				guard candidate.count == sequence.count + 1, reachable.contains(candidate), let next = candidate.last else { return nil }
				return KeyCompletion(next: next, sequence: candidate, action: row.action, label: row.label, isInItemMenu: row.isInItemMenu)
			}
		}
	}

	private func clearPending() {
		pending = []
		pendingSince = nil
		completions = []
	}

	private func rebuild() {
		rows = keymap.table(for: scheme)
		bound = keymap.sequences(for: scheme)
	}
}
