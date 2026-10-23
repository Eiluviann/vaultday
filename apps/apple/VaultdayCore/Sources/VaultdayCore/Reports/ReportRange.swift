import Foundation

/// The ranges the Reports tab offers: three presets and a custom one.
public enum ReportPreset: String, CaseIterable, Identifiable, Sendable {
	case thisWeek = "This week"
	case lastWeek = "Last week"
	case thisMonth = "This month"
	case custom = "Custom"

	public var id: String { rawValue }
}

/// A report's days, `YYYY-MM-DD` both ends, inclusive.
public struct ReportRange: Equatable, Sendable {
	public let from: String
	public let to: String

	public init(from: String, to: String) {
		self.from = from
		self.to = to
	}

	/// Whether the range runs forwards (a one-day range does).
	public var isValid: Bool {
		guard let first = DayText.date(from: from), let last = DayText.date(from: to) else { return false }
		return first <= last
	}

	/// The preset's days around `now`; weeks run Monday to Sunday. `nil` for
	/// Custom, whose days the person picks.
	public static func range(for preset: ReportPreset, now: Date, calendar: Calendar = .current) -> ReportRange? {
		var monday = calendar
		monday.firstWeekday = 2
		switch preset {
		case .thisWeek:
			return week(containing: now, calendar: monday)
		case .lastWeek:
			guard let before = monday.date(byAdding: .day, value: -7, to: now) else { return nil }
			return week(containing: before, calendar: monday)
		case .thisMonth:
			guard let month = monday.dateInterval(of: .month, for: now),
			      let last = monday.date(byAdding: .day, value: -1, to: month.end)
			else { return nil }
			return ReportRange(from: DayText.string(month.start), to: DayText.string(last))
		case .custom:
			return nil
		}
	}

	private static func week(containing day: Date, calendar: Calendar) -> ReportRange? {
		guard let week = calendar.dateInterval(of: .weekOfYear, for: day),
		      let last = calendar.date(byAdding: .day, value: -1, to: week.end)
		else { return nil }
		return ReportRange(from: DayText.string(week.start), to: DayText.string(last))
	}
}
