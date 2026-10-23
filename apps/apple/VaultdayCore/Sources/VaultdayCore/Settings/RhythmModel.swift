import Foundation
import Observation

/// A ritual that runs at a time of day, and the time and day it has until the user sets one.
public enum Ritual: String, CaseIterable, Identifiable, Hashable, Sendable {
	case planning
	case shutdown
	case weeklyReview = "weekly-review"
	case lightPlanning = "light-planning"
	case projectReview = "project-review"

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .planning: "Morning planning"
		case .shutdown: "Shutdown"
		case .weeklyReview: "Weekly review"
		case .lightPlanning: "Light planning"
		case .projectReview: "Project review"
		}
	}

	/// The time shown until the user sets one.
	public var defaultTime: DayTime {
		let minutes = switch self {
		case .planning: 8 * 60 + 45
		case .shutdown: 17 * 60 + 30
		case .weeklyReview: 16 * 60
		case .lightPlanning: 18 * 60 + 30
		case .projectReview: 18 * 60
		}
		return DayTime(minuteOfDay: minutes) ?? DayTime(minuteOfDay: 0)!
	}

	/// The weekday of a weekly ritual, `nil` for a daily one.
	public var defaultDay: WeekdayChoice? {
		switch self {
		case .weeklyReview: .friday
		case .projectReview: .sunday
		case .planning, .shutdown, .lightPlanning: nil
		}
	}

	/// Whether the ritual runs on one day of the week.
	public var isWeekly: Bool { defaultDay != nil }
}

/// How the day runs: ritual times, working hours, the workload threshold, the
/// break pattern, roll-over and where a new task lands. The Rituals and
/// routines step and Settings share it, so a choice made in one is
/// what the other shows. Every change is checked, then written through the
/// core to the app-level document. Rituals do not run in the base version:
/// their times are stored for when they do.
@MainActor
@Observable
public final class RhythmModel {
	public static let minimumWorkloadMinutes = 15
	public static let maximumWorkloadMinutes = 24 * 60
	public static let minimumLongTimerMinutes = 15
	public static let maximumLongTimerMinutes = 24 * 60

	public private(set) var breakPattern: BreakPatternSetting = .fiftyTen
	public private(set) var rolloverPosition: ListPositionSetting = .top
	public private(set) var newTaskPosition: ListPositionSetting = .bottom
	public private(set) var overrunWarning: OverrunWarningSetting = .intoFreeTime
	public private(set) var workloadThresholdMinutes = 8 * 60
	/// How long a session may run before the long-timer check asks "still working on this?".
	public private(set) var longTimerLimitMinutes = 3 * 60
	public private(set) var workingHours = WorkingHoursSetting(
		kind: .fixed,
		start: DayTime(minuteOfDay: 9 * 60) ?? DayTime(minuteOfDay: 0)!,
		end: DayTime(minuteOfDay: 17 * 60 + 30) ?? DayTime(minuteOfDay: 0)!,
		days: [.monday, .tuesday, .wednesday, .thursday, .friday],
	)
	public private(set) var planAfterWorkForLife = true
	public private(set) var remindersRing = true
	public private(set) var times: [Ritual: DayTime] = [:]
	public private(set) var days: [Ritual: WeekdayChoice] = [:]
	/// One message per field the last edit refused, by field name.
	public private(set) var issues: [String: String] = [:]
	public private(set) var errorMessage: String?

	private let session: CoreSession

	public init(session: CoreSession) {
		self.session = session
		reload()
	}

	/// Reads the document again.
	public func reload() {
		let appState = session.appState
		breakPattern = BreakPatternSetting(appState.breakPattern())
		rolloverPosition = ListPositionSetting(appState.rolloverPosition())
		newTaskPosition = ListPositionSetting(appState.newTaskPosition())
		overrunWarning = OverrunWarningSetting(appState.overrunWarning())
		workloadThresholdMinutes = Int(appState.workloadThresholdMinutes())
		longTimerLimitMinutes = Int(appState.longTimerLimitMinutes())
		let hours = appState.workingHours()
		if let start = DayTime(minuteOfDay: Int(hours.startMinute)), let end = DayTime(minuteOfDay: Int(hours.endMinute)) {
			workingHours = WorkingHoursSetting(kind: WorkingHoursKindSetting(hours.kind), start: start, end: end, days: Set(hours.days.map(WeekdayChoice.init)))
		}
		planAfterWorkForLife = appState.plansLifeAfterWork()
		remindersRing = appState.reminderTimesRing()
		times = [:]
		days = [:]
		for ritual in Ritual.allCases {
			times[ritual] = appState.ritualMinute(ritual: ritual.rawValue).flatMap { DayTime(minuteOfDay: Int($0)) } ?? ritual.defaultTime
			days[ritual] = appState.ritualDay(ritual: ritual.rawValue).map(WeekdayChoice.init) ?? ritual.defaultDay
		}
	}

	// MARK: Rituals

	public func time(of ritual: Ritual) -> DayTime { times[ritual] ?? ritual.defaultTime }

	public func day(of ritual: Ritual) -> WeekdayChoice? { days[ritual] ?? ritual.defaultDay }

	/// Sets when a ritual runs from the text of its field; a time that is not on the clock is refused and nothing is written.
	@discardableResult
	public func setTime(_ text: String, of ritual: Ritual) -> Bool {
		guard let time = DayTime(text: text) else {
			issues[ritual.rawValue] = "Use a time like 08:45"
			return false
		}
		issues[ritual.rawValue] = nil
		return write { try $0.setRitualMinute(ritual: ritual.rawValue, minute: UInt32(time.minuteOfDay)) }
	}

	/// Sets the weekday of a weekly ritual.
	public func setDay(_ day: WeekdayChoice, of ritual: Ritual) {
		guard ritual.isWeekly else { return }
		write { try $0.setRitualDay(ritual: ritual.rawValue, day: day.core) }
	}

	// MARK: Working hours

	public func setWorkingHoursKind(_ kind: WorkingHoursKindSetting) {
		var hours = workingHours
		hours.kind = kind
		store(hours)
	}

	/// Sets the working hours from the text of both fields. Fixed hours end after they start.
	@discardableResult
	public func setWorkingHours(start startText: String, end endText: String) -> Bool {
		guard let start = DayTime(text: startText), let end = DayTime(text: endText) else {
			issues["workingHours"] = "Use times like 09:00"
			return false
		}
		var hours = workingHours
		hours.start = start
		hours.end = end
		return store(hours)
	}

	public func setWorkDay(_ day: WeekdayChoice, isOn: Bool) {
		var hours = workingHours
		if isOn { hours.days.insert(day) } else { hours.days.remove(day) }
		store(hours)
	}

	@discardableResult
	private func store(_ hours: WorkingHoursSetting) -> Bool {
		if hours.kind == .fixed, hours.end <= hours.start {
			issues["workingHours"] = "Work has to end after it starts"
			return false
		}
		if hours.days.isEmpty {
			issues["workingHours"] = "Pick at least one working day"
			return false
		}
		issues["workingHours"] = nil
		let view = WorkingHoursView(
			kind: hours.kind.core,
			startMinute: UInt32(hours.start.minuteOfDay),
			endMinute: UInt32(hours.end.minuteOfDay),
			days: WeekdayChoice.allCases.filter { hours.days.contains($0) }.map(\.core),
		)
		return write { try $0.setWorkingHours(hours: view) }
	}

	// MARK: The rest of the day

	public func setBreakPattern(_ pattern: BreakPatternSetting) {
		write { try $0.setBreakPattern(pattern: pattern.core) }
	}

	public func setRolloverPosition(_ position: ListPositionSetting) {
		write { try $0.setRolloverPosition(position: position.core) }
	}

	public func setNewTaskPosition(_ position: ListPositionSetting) {
		write { try $0.setNewTaskPosition(position: position.core) }
	}

	public func setOverrunWarning(_ warning: OverrunWarningSetting) {
		write { try $0.setOverrunWarning(warning: warning.core) }
	}

	public func setPlanAfterWorkForLife(_ isOn: Bool) {
		write { try $0.setPlansLifeAfterWork(isOn: isOn) }
	}

	public func setRemindersRing(_ isOn: Bool) {
		write { try $0.setReminderTimesRing(isOn: isOn) }
	}

	/// The limit the long-timer check uses: the long-timer limit or the
	/// workload threshold, whichever comes first.
	public var longTimerCheckMinutes: Int {
		min(longTimerLimitMinutes, workloadThresholdMinutes)
	}

	/// Sets how long a session may run before the long-timer check asks; 15 minutes to a full day.
	@discardableResult
	public func setLongTimerLimit(minutes: Int) -> Bool {
		guard (Self.minimumLongTimerMinutes ... Self.maximumLongTimerMinutes).contains(minutes) else {
			issues["longTimer"] = "Between 15 minutes and 24 hours"
			return false
		}
		issues["longTimer"] = nil
		return write { try $0.setLongTimerLimitMinutes(minutes: UInt32(minutes)) }
	}

	/// Sets the planned work a day may hold before planning warns, in minutes; 15 minutes to a full day.
	@discardableResult
	public func setWorkloadThreshold(minutes: Int) -> Bool {
		guard (Self.minimumWorkloadMinutes ... Self.maximumWorkloadMinutes).contains(minutes) else {
			issues["workload"] = "Between 15 minutes and 24 hours"
			return false
		}
		issues["workload"] = nil
		return write { try $0.setWorkloadThresholdMinutes(minutes: UInt32(minutes)) }
	}

	@discardableResult
	private func write(_ edit: (AppStateHandle) throws -> Void) -> Bool {
		defer { reload() }
		do {
			try session.edit(edit)
			errorMessage = nil
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}
}
