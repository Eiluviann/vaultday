import Foundation

/// A time of day, to the minute, as the rituals and working hours use it.
public struct DayTime: Hashable, Comparable, Sendable {
	public let minuteOfDay: Int

	public init?(minuteOfDay: Int) {
		guard (0 ..< 24 * 60).contains(minuteOfDay) else { return nil }
		self.minuteOfDay = minuteOfDay
	}

	/// Reads `8:45`, `08:45` or `17:30`; anything else, or a time that is not
	/// on the clock (`24:00`, `8:60`), is refused.
	public init?(text: String) {
		let parts = text.trimmingCharacters(in: .whitespaces).split(separator: ":", omittingEmptySubsequences: false)
		guard parts.count == 2,
		      parts[0].count <= 2, parts[1].count == 2,
		      let hour = Int(parts[0]), let minute = Int(parts[1]),
		      (0 ..< 24).contains(hour), (0 ..< 60).contains(minute)
		else { return nil }
		self.init(minuteOfDay: hour * 60 + minute)
	}

	/// `HH:MM`.
	public var text: String {
		String(format: "%02d:%02d", minuteOfDay / 60, minuteOfDay % 60)
	}

	public static func < (lhs: DayTime, rhs: DayTime) -> Bool { lhs.minuteOfDay < rhs.minuteOfDay }
}

/// A day of the week, Monday first.
public enum WeekdayChoice: String, CaseIterable, Identifiable, Hashable, Sendable {
	case monday, tuesday, wednesday, thursday, friday, saturday, sunday

	public var id: String { rawValue }

	/// Two letters, as the board's day ticks.
	public var shortTitle: String {
		switch self {
		case .monday: "Mo"
		case .tuesday: "Tu"
		case .wednesday: "We"
		case .thursday: "Th"
		case .friday: "Fr"
		case .saturday: "Sa"
		case .sunday: "Su"
		}
	}

	/// Three letters, as the day field's menu.
	public var title: String {
		switch self {
		case .monday: "Mon"
		case .tuesday: "Tue"
		case .wednesday: "Wed"
		case .thursday: "Thu"
		case .friday: "Fri"
		case .saturday: "Sat"
		case .sunday: "Sun"
		}
	}

	var core: WeekdayView {
		switch self {
		case .monday: .monday
		case .tuesday: .tuesday
		case .wednesday: .wednesday
		case .thursday: .thursday
		case .friday: .friday
		case .saturday: .saturday
		case .sunday: .sunday
		}
	}

	init(_ view: WeekdayView) {
		switch view {
		case .monday: self = .monday
		case .tuesday: self = .tuesday
		case .wednesday: self = .wednesday
		case .thursday: self = .thursday
		case .friday: self = .friday
		case .saturday: self = .saturday
		case .sunday: self = .sunday
		}
	}
}

/// Focus and break length.
public enum BreakPatternSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case none, fiftyTen, twentyFiveFive

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .none: "None"
		case .fiftyTen: "50 · 10"
		case .twentyFiveFive: "25 · 5"
		}
	}

	var core: BreakPatternView {
		switch self {
		case .none: .none
		case .fiftyTen: .fiftyTen
		case .twentyFiveFive: .twentyFiveFive
		}
	}

	init(_ view: BreakPatternView) {
		switch view {
		case .none: self = .none
		case .fiftyTen: self = .fiftyTen
		case .twentyFiveFive: self = .twentyFiveFive
		}
	}
}

/// Where a task lands in a list: unfinished tasks at roll-over, a new task when it is added.
public enum ListPositionSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case top, bottom

	public var id: String { rawValue }

	public var rolloverTitle: String { self == .top ? "To the top" : "To the bottom" }
	public var newTaskTitle: String { self == .top ? "At the top" : "At the bottom" }

	var core: ListPositionView { self == .top ? .top : .bottom }

	init(_ view: ListPositionView) {
		self = view == .top ? .top : .bottom
	}
}

/// When a day that runs over warns.
public enum OverrunWarningSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case intoFreeTime, never

	public var id: String { rawValue }

	public var title: String { self == .intoFreeTime ? "Into free time" : "Never" }

	var core: OverrunWarningView { self == .intoFreeTime ? .intoFreeTime : .never }

	init(_ view: OverrunWarningView) {
		self = view == .intoFreeTime ? .intoFreeTime : .never
	}
}

/// How the shortcut caps show.
public enum ShortcutCapsSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case animated, instant

	public var id: String { rawValue }

	public var title: String { self == .animated ? "Animated" : "Instant" }

	var core: ShortcutCapsView { self == .animated ? .animated : .instant }

	init(_ view: ShortcutCapsView) {
		self = view == .animated ? .animated : .instant
	}
}

/// Fixed times or a daily target.
public enum WorkingHoursKindSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case fixed, flexible

	public var id: String { rawValue }

	public var title: String { self == .fixed ? "Fixed" : "Flexible" }

	var core: WorkingHoursKindView { self == .fixed ? .fixed : .flexible }

	init(_ view: WorkingHoursKindView) {
		self = view == .fixed ? .fixed : .flexible
	}
}

/// The Work sphere's working hours.
public struct WorkingHoursSetting: Equatable, Sendable {
	public var kind: WorkingHoursKindSetting
	public var start: DayTime
	public var end: DayTime
	public var days: Set<WeekdayChoice>
}
