import Foundation
import Observation

/// One day of the Toggl week, as the Hours slots show it.
public struct TogglWeekDay: Identifiable, Hashable, Sendable {
	public enum State: Hashable, Sendable {
		case pushed, queued, failed, empty
	}

	/// `YYYY-MM-DD`.
	public let date: String
	/// "Mon".
	public let weekday: String
	public let seconds: Int
	/// A flat FTE day rather than session entries.
	public let isFlat: Bool
	public let state: State

	public var id: String { date }

	/// "Mon", the day without its hours, for a shared screen.
	public var titleWithoutHours: String { weekday }

	/// "Mon · 8h flat", "Tue · 1h 30m".
	public var title: String {
		let amount = DurationText.minutes(seconds / 60)
		return "\(weekday) · \(amount)\(isFlat ? " flat" : "")"
	}

	/// "Pushed", "Queued", "Failed".
	public var stateText: String {
		switch state {
		case .pushed: "Pushed"
		case .queued: "Queued"
		case .failed: "Failed"
		case .empty: ""
		}
	}

	public init(date: String, weekday: String, seconds: Int, isFlat: Bool, state: State) {
		self.date = date
		self.weekday = weekday
		self.seconds = seconds
		self.isFlat = isFlat
		self.state = state
	}
}

/// What a source answers for one week.
public struct TogglWeekSnapshot: Equatable, Sendable {
	/// Whether the feature is on for the workspace.
	public let isConnected: Bool
	public let days: [TogglWeekDay]

	public init(isConnected: Bool, days: [TogglWeekDay]) {
		self.isConnected = isConnected
		self.days = days
	}
}

/// Where the Toggl week comes from: the core in the app, a fake in tests.
public protocol TogglWeekSource: AnyObject {
	/// The week from `weekStart` (`YYYY-MM-DD`, a Monday), of one story or the whole workspace.
	func togglWeek(startingOn weekStart: String, story: String?) throws -> TogglWeekSnapshot
}

/// The "Toggl · this week" slot of both Hours tabs: the days that hold
/// time for Toggl and whether it is pushed, queued or failed, read from the
/// queue in the workspace document. "Toggl not connected" while the feature
/// is off. Reads no network.
@MainActor
@Observable
public final class TogglWeekModel {
	public static let title = "Toggl · this week"
	public static let notConnectedText = "Toggl not connected"
	public static let emptyText = "Nothing for Toggl this week yet"

	public private(set) var isConnected = false
	public private(set) var days: [TogglWeekDay] = []
	public private(set) var errorMessage: String?

	public init() {}

	/// The days with time for Toggl, in order.
	public var visibleDays: [TogglWeekDay] {
		days.filter { $0.state != .empty }
	}

	/// The line under the days: what the slot says when it shows no day.
	public var statusText: String? {
		guard isConnected else { return Self.notConnectedText }
		return visibleDays.isEmpty ? Self.emptyText : nil
	}

	/// Reads the week holding `day`, for `story` or the whole workspace.
	public func load(source: (any TogglWeekSource)?, containing day: Date, story: String?, calendar: Calendar = .current) {
		guard let source else {
			clear()
			return
		}
		do {
			let snapshot = try source.togglWeek(startingOn: Self.weekStart(of: day, calendar: calendar), story: story)
			isConnected = snapshot.isConnected
			days = snapshot.days
			errorMessage = nil
		} catch {
			isConnected = false
			days = []
			errorMessage = coreErrorMessage(error)
		}
	}

	public func clear() {
		isConnected = false
		days = []
		errorMessage = nil
	}

	/// The Monday on or before `day`, `YYYY-MM-DD`.
	public static func weekStart(of day: Date, calendar: Calendar = .current) -> String {
		var monday = calendar
		monday.firstWeekday = 2
		let start = monday.dateInterval(of: .weekOfYear, for: day)?.start ?? day
		return DayText.string(start)
	}
}
