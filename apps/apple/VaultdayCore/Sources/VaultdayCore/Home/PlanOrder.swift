import Foundation

/// Reordering the day's list. Only unfinished tasks move; the done ones stay
/// below them in the order the core gave. The result is what the workspace's
/// app state stores for the date.
public enum PlanOrder {
	/// The order after `id` moves to `destination`, an index among the
	/// unfinished tasks as they read before the move. An unknown id, or a
	/// done task, leaves the order as it is.
	public static func moving(_ id: String, to destination: Int, in items: [PlanItem]) -> [String] {
		var unfinished = items.filter { !$0.isDone }.map(\.taskId)
		let done = items.filter(\.isDone).map(\.taskId)
		guard let from = unfinished.firstIndex(of: id) else { return unfinished + done }
		unfinished.remove(at: from)
		let clamped = min(max(destination, 0), unfinished.count)
		unfinished.insert(id, at: clamped)
		return unfinished + done
	}

	/// The order after `id` moves one place up (`-1`) or down (`+1`).
	public static func stepping(_ id: String, by step: Int, in items: [PlanItem]) -> [String] {
		let unfinished = items.filter { !$0.isDone }.map(\.taskId)
		guard let from = unfinished.firstIndex(of: id) else { return items.map(\.taskId) }
		return moving(id, to: from + step, in: items)
	}

	/// The destination index for a drop: SwiftUI's `onMove` reports the gap
	/// before which the rows land, counted with the moved row still in place.
	public static func destination(forDropBefore gap: Int, from source: Int) -> Int {
		gap > source ? gap - 1 : gap
	}
}
