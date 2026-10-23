import Foundation
@testable import VaultdayCore

/// The day's plan without a vault, for the register and the storyboards.
/// A day's list is its stored order of the tasks scheduled on it, then any
/// scheduled task the order does not name yet.
@MainActor
final class FakePlan: PlanEditing {
	var shownDate = "2026-10-23"
	var todayDate = "2026-10-23"
	var lines: [String: TaskLineState] = [:]
	var orders: [String: [String]] = [:]
	/// Ids whose edits the fake refuses, as the core would refuse a changed line.
	var refusing: Set<String> = []
	private(set) var refreshCount = 0
	/// Task ids in the order they were added, so appended ones keep it.
	private var created: [String] = []

	func add(_ taskId: String, _ text: String, scheduled: String?) {
		lines[taskId] = TaskLineState(text: text, scheduled: scheduled)
		created.append(taskId)
	}

	var shownList: [String] { listOrder(on: shownDate) }

	func lineState(of taskId: String) -> TaskLineState? {
		lines[taskId]
	}

	func apply(_ edit: TaskLineEdit, to taskId: String) -> LineWrite {
		guard var line = lines[taskId] else { return .refused("no task with [id:: \(taskId)]") }
		guard !refusing.contains(taskId) else { return .refused("the line changed while it was written") }
		let old = Self.text(of: line)
		switch edit {
		case let .done(day): line.completion = day
		case .reopen: line.completion = nil
		case let .schedule(day): line.scheduled = day
		case let .due(day): line.due = day
		case let .estimate(estimate): line.estimate = estimate
		case let .text(text): line.text = text
		}
		lines[taskId] = line
		return .written(old: old, new: Self.text(of: line))
	}

	func restore(_ taskId: String, line: String, expecting current: String) -> String? {
		guard let state = lines[taskId], Self.text(of: state) == current else { return "the line changed since" }
		guard let restored = Self.state(from: line) else { return "not a task line" }
		lines[taskId] = restored
		return nil
	}

	/// The fake's line: its fields joined, so a restore can read them back.
	static func text(of state: TaskLineState) -> String {
		[state.text, state.scheduled ?? "", state.due ?? "", state.estimate ?? "", state.completion ?? ""].joined(separator: "|")
	}

	static func state(from text: String) -> TaskLineState? {
		let parts = text.split(separator: "|", omittingEmptySubsequences: false).map(String.init)
		guard parts.count == 5 else { return nil }
		let field = { (index: Int) -> String? in parts[index].isEmpty ? nil : parts[index] }
		return TaskLineState(text: parts[0], scheduled: field(1), due: field(2), estimate: field(3), completion: field(4))
	}

	func listOrder(on date: String) -> [String] {
		let scheduled = created.filter { lines[$0]?.scheduled == date }
		let stored = (orders[date] ?? []).filter { scheduled.contains($0) }
		return stored + scheduled.filter { !stored.contains($0) }
	}

	func storeOrder(_ taskIds: [String], on date: String) -> String? {
		orders[date] = taskIds
		return nil
	}

	func refresh() {
		refreshCount += 1
	}
}
