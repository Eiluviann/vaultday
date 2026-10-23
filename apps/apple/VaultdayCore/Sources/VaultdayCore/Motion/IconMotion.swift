import Foundation

/// The four SF Symbols draw effects of the design note's icon motion
/// (decided 2026-10-16): an icon draws on when it appears, draws off when
/// it leaves, is replaced when it changes state, and draws to a value for
/// progress.
public enum IconMotionKind: Equatable, Sendable {
	case drawOn
	case drawOff
	case replace
	case variableDraw
}

/// What one icon does for one change.
public enum IconMotionPlan: Equatable, Sendable {
	/// No animation: the first render, or a change already played.
	case none
	/// Reduce Motion: the plain crossfade the system gives, never a draw.
	case crossfade
	/// The draw effect.
	case effect(IconMotionKind)
}

/// The rules every animated icon follows, kept in one value so each view
/// enforces them the same way:
///
/// - never on first render: what is on screen when a view first shows is
///   simply there, so a list never draws itself in;
/// - one effect per change: a change, named by its value, plays once,
///   however often the view is evaluated, and only its own effect (no
///   bounce or pulse stacked on a draw);
/// - Reduce Motion gets a crossfade instead of any draw.
public struct IconMotionGate<Value: Equatable & Sendable>: Equatable, Sendable {
	/// The value shown last; `nil` before the first render.
	public private(set) var shownValue: Value?
	public private(set) var hasRendered = false
	/// How many changes played an effect or a crossfade.
	public private(set) var playedCount = 0

	public init() {}

	/// The view's first render with `value`: recorded, never animated.
	public mutating func firstRender(_ value: Value) {
		guard !hasRendered else { return }
		hasRendered = true
		shownValue = value
	}

	/// The plan for a change a view is rendering now, from what it knows at
	/// that moment: whether it has rendered before, whether the value it
	/// shows differs from the last one, and Reduce Motion. The views use
	/// this inside `body`, where state cannot change; ``plan(_:to:reduceMotion:)``
	/// is the same rule over a sequence of values.
	public static func plan(_ kind: IconMotionKind, hasRendered: Bool, isChange: Bool, reduceMotion: Bool) -> IconMotionPlan {
		guard hasRendered, isChange else { return .none }
		return reduceMotion ? .crossfade : .effect(kind)
	}

	/// The plan for the icon moving to `value`: ``IconMotionPlan/none`` on
	/// the first render or when `value` is what is shown already, else
	/// `kind`, or a crossfade under Reduce Motion.
	public mutating func plan(_ kind: IconMotionKind, to value: Value, reduceMotion: Bool) -> IconMotionPlan {
		guard hasRendered else {
			firstRender(value)
			return .none
		}
		let plan = Self.plan(kind, hasRendered: true, isChange: value != shownValue, reduceMotion: reduceMotion)
		shownValue = value
		if plan != .none { playedCount += 1 }
		return plan
	}
}

/// How long icon motion that is ours (not the symbol's own effect) lasts:
/// the crossfade that stands in for a draw under Reduce Motion, and the
/// transaction that carries a replace. The design system's UI transition,
/// 150 ms ease-out (`motion-transition` in design/tokens.json).
public enum IconMotionTiming {
	public static let crossfade: TimeInterval = 0.15
}

/// Which rows of a list arrived while the list was on screen, so only
/// their marks draw on: the rows there at the list's first render are
/// simply there.
public struct ArrivalLedger: Equatable, Sendable {
	public private(set) var knownIds: Set<String> = []
	public private(set) var hasSettled = false

	public init() {}

	/// Whether the row `id` arrived after the list first showed.
	public func isArrival(_ id: String) -> Bool {
		hasSettled && !knownIds.contains(id)
	}

	/// The list's first render: its rows are known, none an arrival.
	public mutating func settle(_ ids: [String]) {
		guard !hasSettled else { return }
		hasSettled = true
		knownIds = Set(ids)
	}

	/// The rows shown now; arrivals become known once they have been drawn.
	public mutating func record(_ ids: [String]) {
		knownIds.formUnion(ids)
	}
}
