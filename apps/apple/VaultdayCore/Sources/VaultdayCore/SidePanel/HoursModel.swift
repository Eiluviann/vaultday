import Foundation
import Observation

/// One story's row of the Hours tab.
public struct HoursRow: Identifiable, Hashable, Sendable {
	public let number: String
	public let title: String
	/// Real timer time, private: it never leaves the Mac in a report.
	public let trackedMinutes: Int
	public let estimateMinutes: Int?
	public let remainingMinutes: Int
	public let overtimeMinutes: Int
	/// The budget is the sum of the tasks' estimates, not the story's own.
	public let isEstimateFromTasks: Bool

	public var id: String { number }
	public var isOver: Bool { overtimeMinutes > 0 }

	/// Tracked against the estimate, 0 to 1, for the bar; `nil` without an estimate.
	public var share: Double? {
		guard let estimateMinutes, estimateMinutes > 0 else { return nil }
		return min(1, Double(trackedMinutes) / Double(estimateMinutes))
	}

	/// "Estimate 6h · 3h 55m left", "Estimate 1h · 15m over", "No estimate".
	public var budgetText: String {
		guard let estimateMinutes else { return "No estimate" }
		let estimate = "Estimate \(DurationText.minutes(estimateMinutes))\(isEstimateFromTasks ? " from tasks" : "")"
		let rest = isOver ? "\(DurationText.minutes(overtimeMinutes)) over" : "\(DurationText.minutes(remainingMinutes)) left"
		return "\(estimate) · \(rest)"
	}
}

/// The Hours tab: today's tracked time against the daily target from the
/// workspace's `hours_per_day`, tracked, estimate and remaining per open
/// story, and the Toggl week's slot. The maths is the core's.
@MainActor
@Observable
public final class HoursModel {
	public private(set) var targetMinutes = 0
	public private(set) var trackedMinutes = 0
	public private(set) var rows: [HoursRow] = []
	public private(set) var errorMessage: String?
	/// The "Toggl · this week" slot: the week holding the loaded day.
	public let togglWeek = TogglWeekModel()

	public init() {}

	/// Today's tracked time against the target, 0 to 1.
	public var targetShare: Double {
		guard targetMinutes > 0 else { return 0 }
		return min(1, Double(trackedMinutes) / Double(targetMinutes))
	}

	/// "3h 10m of 8h".
	public var targetText: String {
		"\(DurationText.minutes(trackedMinutes)) of \(DurationText.minutes(targetMinutes))"
	}

	/// Reads the Hours of `date` (`YYYY-MM-DD`) for the open workspace.
	public func load(from workspace: WorkspaceModel, date: String) {
		guard let vault = workspace.vault, let state = workspace.state else {
			clear()
			return
		}
		do {
			let view = try state.hours(workspace: vault, date: date)
			targetMinutes = Int(view.targetMinutes)
			trackedMinutes = Int(view.trackedMinutes)
			rows = view.stories.map(HoursRow.init)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		togglWeek.load(source: workspace.togglWeekSource, containing: DayText.date(from: date) ?? Date(), story: nil)
	}

	public func clear() {
		targetMinutes = 0
		trackedMinutes = 0
		rows = []
		errorMessage = nil
		togglWeek.clear()
	}
}

extension HoursRow {
	init(_ view: StoryHoursView) {
		self.init(
			number: view.number,
			title: view.title,
			trackedMinutes: Int(view.trackedMinutes),
			estimateMinutes: view.estimateMinutes.map(Int.init),
			remainingMinutes: Int(view.remainingMinutes),
			overtimeMinutes: Int(view.overtimeMinutes),
			isEstimateFromTasks: view.budget == .tasks,
		)
	}
}
