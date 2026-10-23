import CoreGraphics
import Foundation
import Observation

/// One task the menu-bar extra offers to start.
public struct TimerPick: Identifiable, Hashable, Sendable {
	public var id: String { taskId }
	public let taskId: String
	public let text: String
	public let story: String?
	/// Recently timed, so listed first.
	public let isRecent: Bool

	public init(taskId: String, text: String, story: String?, isRecent: Bool) {
		self.taskId = taskId
		self.text = text
		self.story = story
		self.isRecent = isRecent
	}
}

/// The menu-bar extra's list, Timery style: recent tasks first, then the
/// day's open ones; ↑ and ↓ move, ⏎ starts the selected one, ⌘N types a
/// new entry that the same matching as `vaultday timer start` resolves.
@MainActor
@Observable
public final class TimerPickerModel {
	public private(set) var picks: [TimerPick] = []
	public private(set) var selectedIndex = 0
	/// ⌘N: a typed entry instead of the list.
	public private(set) var isComposing = false
	public var draft = ""

	public init() {}

	/// Fills the list: tasks the timer ran on recently, then the day's open tasks.
	public func load(recentTaskIds: [String], items: [PlanItem], limit: Int = 8) {
		var seen: Set<String> = []
		var rows: [TimerPick] = []
		let byId = Dictionary(items.map { ($0.taskId, $0) }, uniquingKeysWith: { first, _ in first })
		for taskId in recentTaskIds {
			guard let item = byId[taskId], !item.isDone, seen.insert(taskId).inserted else { continue }
			rows.append(TimerPick(taskId: taskId, text: item.text, story: item.storyNumber, isRecent: true))
		}
		for item in items where !item.isDone && seen.insert(item.taskId).inserted {
			rows.append(TimerPick(taskId: item.taskId, text: item.text, story: item.storyNumber, isRecent: false))
		}
		picks = Array(rows.prefix(limit))
		selectedIndex = min(selectedIndex, max(0, picks.count - 1))
	}

	public var selectedPick: TimerPick? {
		picks.indices.contains(selectedIndex) ? picks[selectedIndex] : nil
	}

	/// ↑ is -1, ↓ is +1; the selection stops at either end.
	public func moveSelection(by offset: Int) {
		guard !picks.isEmpty else { return }
		selectedIndex = min(max(0, selectedIndex + offset), picks.count - 1)
	}

	public func select(_ pick: TimerPick) {
		if let index = picks.firstIndex(of: pick) { selectedIndex = index }
	}

	/// ⌘N.
	public func beginNewEntry() {
		isComposing = true
		draft = ""
	}

	public func endNewEntry() {
		isComposing = false
		draft = ""
	}
}

/// Where the mini tracker sits and when it shows. Pure, so it is tested
/// without a window.
public enum MiniTrackerLayout {
	/// The collapsed bar: workspace dot, task, ids, elapsed time, pause, stop.
	public static let collapsedSize = CGSize(width: 400, height: 56)
	/// The expanded form adds the subtasks, the progress against the
	/// estimate and the actions.
	public static let expandedSize = CGSize(width: 400, height: 236)
	/// The gap kept to the screen edge it snaps to.
	public static let edgeMargin: CGFloat = 12

	/// The origin after a drag ends: inside the visible frame, moved to the
	/// nearest of its four edges along one axis.
	public static func snappedOrigin(for frame: CGRect, in visible: CGRect, margin: CGFloat = edgeMargin) -> CGPoint {
		let minX = visible.minX + margin
		let maxX = visible.maxX - margin - frame.width
		let minY = visible.minY + margin
		let maxY = visible.maxY - margin - frame.height
		var origin = CGPoint(x: clamp(frame.minX, minX, maxX), y: clamp(frame.minY, minY, maxY))
		let distances: [(CGFloat, Edge)] = [
			(origin.x - minX, .left),
			(maxX - origin.x, .right),
			(origin.y - minY, .bottom),
			(maxY - origin.y, .top),
		]
		switch distances.min(by: { $0.0 < $1.0 })?.1 {
		case .left: origin.x = minX
		case .right: origin.x = maxX
		case .bottom: origin.y = minY
		case .top: origin.y = maxY
		case nil: break
		}
		return origin
	}

	/// The bar shows while a session is open and the tracker is on, and
	/// hides while Vaultday's own main window is frontmost.
	public static func isVisible(hasOpenSession: Bool, isEnabled: Bool, isMainWindowFrontmost: Bool) -> Bool {
		hasOpenSession && isEnabled && !isMainWindowFrontmost
	}

	private enum Edge { case left, right, bottom, top }

	private static func clamp(_ value: CGFloat, _ lower: CGFloat, _ upper: CGFloat) -> CGFloat {
		upper < lower ? lower : min(max(value, lower), upper)
	}
}
