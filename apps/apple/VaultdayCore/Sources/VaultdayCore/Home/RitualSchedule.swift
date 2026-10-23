import Foundation

/// One row of the sidebar's Rituals list. Rituals do not run in the base
/// version, so a row only shows when the ritual is next due, from the times
/// stored in Settings; it never shows a ritual as done.
public struct RitualRow: Identifiable, Hashable, Sendable {
	public var id: String { ritual.rawValue }
	public let ritual: Ritual
	public let title: String
	/// "17:30" for a daily ritual, "Fri 16:00" for a weekly one.
	public let whenText: String
	/// When it is next due.
	public let next: Date
	/// The soonest of the rows.
	public let isNext: Bool
}

/// The sidebar's rituals and when each is next due.
public enum RitualSchedule {
	/// The rituals a sphere keeps, in the Rituals and routines step's order.
	public static func rituals(for sphere: WorkspaceSphere) -> [Ritual] {
		switch sphere {
		case .work: [.planning, .shutdown, .weeklyReview]
		case .life: [.lightPlanning, .projectReview]
		}
	}

	/// The rows for `rituals` at `now`, the soonest marked next.
	public static func rows(
		_ rituals: [Ritual],
		time: (Ritual) -> DayTime,
		day: (Ritual) -> WeekdayChoice?,
		now: Date,
		calendar: Calendar = .current,
	) -> [RitualRow] {
		let dues = rituals.map { ritual in
			(ritual, nextOccurrence(of: time(ritual), on: day(ritual), after: now, calendar: calendar))
		}
		let soonest = dues.map(\.1).min()
		return dues.map { ritual, next in
			let weekday = day(ritual)
			return RitualRow(
				ritual: ritual,
				title: ritual.title,
				whenText: weekday.map { "\($0.title) \(time(ritual).text)" } ?? time(ritual).text,
				next: next,
				isNext: next == soonest,
			)
		}
	}

	/// The first time at or after `now` the ritual is due: today or tomorrow
	/// for a daily one, the next `weekday` for a weekly one.
	public static func nextOccurrence(of time: DayTime, on weekday: WeekdayChoice?, after now: Date, calendar: Calendar = .current) -> Date {
		var parts = DateComponents()
		parts.hour = time.minuteOfDay / 60
		parts.minute = time.minuteOfDay % 60
		if let weekday {
			parts.weekday = calendarWeekday(weekday)
		}
		let startOfMinute = calendar.dateInterval(of: .minute, for: now)?.start ?? now
		if calendar.date(startOfMinute, matchesComponents: parts) {
			return startOfMinute
		}
		return calendar.nextDate(after: now, matching: parts, matchingPolicy: .nextTime) ?? now
	}

	/// Foundation counts weekdays from Sunday, 1 to 7.
	private static func calendarWeekday(_ day: WeekdayChoice) -> Int {
		switch day {
		case .sunday: 1
		case .monday: 2
		case .tuesday: 3
		case .wednesday: 4
		case .thursday: 5
		case .friday: 6
		case .saturday: 7
		}
	}
}
