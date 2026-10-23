import Foundation

/// The story screen's panel tabs below the Story block, in rail order.
/// Reports arrives with PR 18 of Phase 2 and Claude Code later; both are
/// Planned in the base version.
public enum StoryPanelTab: String, CaseIterable, Identifiable, Sendable {
	case hours = "Hours"
	case reports = "Reports"
	case claudeCode = "Claude Code"

	public var id: String { rawValue }

	public var isPlanned: Bool { self != .hours }

	/// The SF Symbol on the rail.
	public var symbol: String {
		switch self {
		case .hours: "gauge.medium"
		case .reports: "doc.richtext"
		case .claudeCode: "terminal"
		}
	}

	/// The tab `step` places along the rail, wrapping at either end.
	public func stepped(by step: Int) -> StoryPanelTab {
		let tabs = Self.allCases
		let index = tabs.firstIndex(of: self) ?? 0
		return tabs[((index + step) % tabs.count + tabs.count) % tabs.count]
	}

	/// The tab at `place` on the rail, counted from 1.
	public static func at(place: Int) -> StoryPanelTab? {
		allCases.indices.contains(place - 1) ? allCases[place - 1] : nil
	}
}

/// The story's three numbers, kept apart: tracked (the timer's real time,
/// private), logged against the estimate, and what remains of it.
public struct StoryHours: Equatable, Sendable {
	public let trackedMinutes: Int
	public let loggedMinutes: Int
	public let estimateMinutes: Int?
	/// The estimate less the logged time.
	public let remainingMinutes: Int
	public let overtimeMinutes: Int
	public let isEstimateFromTasks: Bool

	public init(trackedMinutes: Int, loggedMinutes: Int, estimateMinutes: Int?, remainingMinutes: Int, overtimeMinutes: Int, isEstimateFromTasks: Bool) {
		self.trackedMinutes = trackedMinutes
		self.loggedMinutes = loggedMinutes
		self.estimateMinutes = estimateMinutes
		self.remainingMinutes = remainingMinutes
		self.overtimeMinutes = overtimeMinutes
		self.isEstimateFromTasks = isEstimateFromTasks
	}

	public var trackedText: String { DurationText.minutes(trackedMinutes) }

	public var loggedText: String { DurationText.minutes(loggedMinutes) }

	/// "6h", "1h 30m from tasks", "No estimate".
	public var estimateText: String {
		guard let estimateMinutes else { return "No estimate" }
		return DurationText.minutes(estimateMinutes) + (isEstimateFromTasks ? " from tasks" : "")
	}

	/// "4h", "30m over", "No estimate".
	public var remainingText: String {
		guard estimateMinutes != nil else { return "No estimate" }
		return overtimeMinutes > 0 ? "\(DurationText.minutes(overtimeMinutes)) over" : DurationText.minutes(remainingMinutes)
	}

	/// Logged against the estimate, 0 to 1, for the bar; `nil` without an estimate.
	public var loggedShare: Double? {
		guard let estimateMinutes, estimateMinutes > 0 else { return nil }
		return min(1, Double(loggedMinutes) / Double(estimateMinutes))
	}

	public var isOver: Bool { overtimeMinutes > 0 }
}
