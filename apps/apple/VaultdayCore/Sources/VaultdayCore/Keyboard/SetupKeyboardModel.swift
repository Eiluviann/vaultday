import CoreGraphics
import Foundation
import Observation

/// A direction of `h` `j` `k` `l`.
public enum SpatialDirection: Sendable {
	case up
	case down
	case left
	case right
}

/// Moves between controls by where they sit on screen: `j` `k` to the
/// nearest control in the row below or above, `h` `l` along the row. The
/// setup's steps are made of cards, switches, chips and fields in no fixed
/// list, so their place is the order.
public enum SpatialNavigation {
	/// The control after `current` in `direction`, by index into `frames`; `nil` when none lies that way.
	public static func next(from current: CGRect, _ direction: SpatialDirection, among frames: [CGRect]) -> Int? {
		let indexed = Array(frames.enumerated())
		switch direction {
		case .down, .up:
			let isDown = direction == .down
			let beyond = indexed.filter { !sharesRow($0.element, current) && (isDown ? $0.element.midY > current.midY : $0.element.midY < current.midY) }
			// The nearest row that way, then the control in it closest across.
			guard let nearest = (isDown ? beyond.min { $0.element.minY < $1.element.minY } : beyond.max { $0.element.maxY < $1.element.maxY }) else { return nil }
			return beyond
				.filter { sharesRow($0.element, nearest.element) }
				.min { abs($0.element.midX - current.midX) < abs($1.element.midX - current.midX) }?
				.offset
		case .left, .right:
			let isRight = direction == .right
			let along = indexed.filter { sharesRow($0.element, current) && (isRight ? $0.element.midX > current.midX : $0.element.midX < current.midX) }
			let nearest = isRight ? along.min { $0.element.minX < $1.element.minX } : along.max { $0.element.maxX < $1.element.maxX }
			return nearest?.offset
		}
	}

	/// The first control in reading order: the top row, then its leftmost.
	public static func first(among frames: [CGRect]) -> Int? {
		Array(frames.enumerated()).min { ($0.element.minY, $0.element.minX) < ($1.element.minY, $1.element.minX) }?.offset
	}

	/// The last control in reading order.
	public static func last(among frames: [CGRect]) -> Int? {
		Array(frames.enumerated()).max { ($0.element.maxY, $0.element.minX) < ($1.element.maxY, $1.element.minX) }?.offset
	}

	/// Whether two controls sit in one row: each one's middle lies within the other's height.
	static func sharesRow(_ left: CGRect, _ right: CGRect) -> Bool {
		(right.minY ... right.maxY).contains(left.midY) || (left.minY ... left.maxY).contains(right.midY)
	}
}

/// The two areas of the setup the keys move between.
public enum SetupArea: Sendable {
	/// The step list on the left.
	case steps
	/// The shown step and its footer.
	case step
}

/// What a key in the setup asks the app to do.
public enum SetupKeyEffect: Equatable, Sendable {
	case none
	/// Press the highlighted control, as a click would.
	case activate(UUID)
	/// Show the step at this place in the step list.
	case openStep(Int)
	/// ⏎ with nothing highlighted: Continue.
	case continueSetup
}

/// Vim motions in the setup flow: two areas, the step list on the left and
/// the shown step with its footer on the right. In the step `j` `k` `h` `l`
/// walk its controls by place and ⏎ presses the highlighted one (or
/// continues when none is); `h` past the left edge enters the step list,
/// where `j` `k` walk the steps reached so far and ⏎ opens one. Esc drops
/// the highlight, one level up.
@MainActor
@Observable
public final class SetupKeyboardModel {
	public private(set) var area: SetupArea = .step
	/// The highlighted control of the step, `nil` while the step itself holds the keys.
	public private(set) var highlighted: UUID?
	/// The highlighted row of the step list.
	public private(set) var stepRow = 0

	/// Where each control of the step sits, in the setup's coordinate space.
	@ObservationIgnored private var frames: [UUID: CGRect] = [:]
	@ObservationIgnored private var stepCount = 0
	@ObservationIgnored private var reachableSteps = 0

	public init() {}

	/// A control appeared or moved.
	public func place(_ id: UUID, at frame: CGRect) {
		frames[id] = frame
	}

	/// A control left the screen; a highlight on it goes with it.
	public func remove(_ id: UUID) {
		frames[id] = nil
		if highlighted == id { highlighted = nil }
	}

	/// The step list: `count` steps, the first `reachable` of them open to a jump, `current` shown.
	public func setSteps(count: Int, reachable: Int, current: Int) {
		stepCount = count
		reachableSteps = max(1, min(reachable, count))
		if area != .steps { stepRow = current }
	}

	/// Whether the highlight is on the control `id`.
	public func isHighlighted(_ id: UUID) -> Bool {
		area == .step && highlighted == id
	}

	/// Whether the highlight is on the step list's row `index`.
	public func isHighlighted(stepRow index: Int) -> Bool {
		area == .steps && stepRow == index
	}

	/// Applies a dispatched action.
	public func apply(_ action: KeyAction) -> SetupKeyEffect {
		switch area {
		case .step: applyInStep(action)
		case .steps: applyInStepList(action)
		}
	}

	private func applyInStep(_ action: KeyAction) -> SetupKeyEffect {
		let ids = Array(frames.keys)
		let rects = ids.map { frames[$0] ?? .zero }
		switch action {
		case .moveDown, .moveUp, .moveLeft, .moveRight:
			let direction: SpatialDirection = switch action {
			case .moveDown: .down
			case .moveUp: .up
			case .moveLeft: .left
			default: .right
			}
			guard let current = highlighted.flatMap({ frames[$0] }) else {
				// From the step itself, `k` starts at the end, every other move at the top.
				let index = direction == .up ? SpatialNavigation.last(among: rects) : SpatialNavigation.first(among: rects)
				highlighted = index.map { ids[$0] }
				return .none
			}
			if let index = SpatialNavigation.next(from: current, direction, among: rects) {
				highlighted = ids[index]
			} else if direction == .left {
				area = .steps
			}
			return .none
		case .moveFirst:
			highlighted = SpatialNavigation.first(among: rects).map { ids[$0] }
			return .none
		case .moveLast:
			highlighted = SpatialNavigation.last(among: rects).map { ids[$0] }
			return .none
		case .open:
			return highlighted.map(SetupKeyEffect.activate) ?? .continueSetup
		case .dismiss:
			highlighted = nil
			return .none
		default:
			return .none
		}
	}

	private func applyInStepList(_ action: KeyAction) -> SetupKeyEffect {
		switch action {
		case .moveDown:
			stepRow = min(stepRow + 1, reachableSteps - 1)
		case .moveUp:
			stepRow = max(stepRow - 1, 0)
		case .moveFirst:
			stepRow = 0
		case .moveLast:
			stepRow = reachableSteps - 1
		case .moveRight, .dismiss:
			area = .step
		case .open:
			area = .step
			highlighted = nil
			return .openStep(stepRow)
		default:
			break
		}
		return .none
	}
}
