import Foundation

/// Week or month.
public enum CloseOutPeriodKind: String, Hashable, Sendable {
	case week, month

	var core: PeriodKindView {
		switch self {
		case .week: .week
		case .month: .month
		}
	}

	init(_ view: PeriodKindView) {
		switch view {
		case .week: self = .week
		case .month: self = .month
		}
	}
}

/// Which steps a workspace's close-out has, by its billing model.
public enum CloseOutBilling: Hashable, Sendable {
	case fte, hourly, fixed
}

/// One of the four steps.
public enum CloseOutStepKind: Hashable, Sendable {
	case toggl, allocate, review, table, close

	/// The SF Symbol of the step.
	public var symbol: String {
		switch self {
		case .toggl: "clock.arrow.circlepath"
		case .allocate: "chart.bar.doc.horizontal"
		case .review: "checklist"
		case .table: "tablecells"
		case .close: "checkmark.seal"
		}
	}
}

/// Where a step stands.
public enum CloseOutStepStatus: Hashable, Sendable {
	case open, done, skipped
}

/// A step of the ritual.
public struct CloseOutStep: Identifiable, Hashable, Sendable {
	public let kind: CloseOutStepKind
	public let title: String
	public let status: CloseOutStepStatus

	public var id: CloseOutStepKind { kind }

	/// Done or skipped: nothing is left to do here.
	public var isSettled: Bool { status != .open }
}

/// What a day of the period is.
public enum CloseOutDayKind: Hashable, Sendable {
	case working, leave, holiday, weekend
}

/// One day of the period.
public struct CloseOutDay: Identifiable, Hashable, Sendable {
	/// `YYYY-MM-DD`.
	public let date: String
	public let kind: CloseOutDayKind

	public var id: String { date }
}

/// One day of the flat Toggl week.
public struct CloseOutFlatDay: Identifiable, Hashable, Sendable {
	public enum State: Hashable, Sendable {
		case notQueued, queued, sent, failed, leave, holiday
	}

	public let date: String
	public let state: State
	public let seconds: Int

	public var id: String { date }

	/// "Mon 19 · 8h", "Wed 21 · leave".
	public var title: String {
		let day = CloseOutText.dayTitle(date)
		switch state {
		case .leave: return "\(day) · leave"
		case .holiday: return "\(day) · holiday"
		default: return "\(day) · \(DurationText.minutes(seconds / 60))"
		}
	}

	/// The state as the row's trailing label; "Dry run" while Toggl is off.
	public func stateText(isTogglOn: Bool) -> String {
		switch state {
		case .notQueued: isTogglOn ? "Not queued" : "Dry run"
		case .queued: "Queued"
		case .sent: "Pushed"
		case .failed: "Failed"
		case .leave, .holiday: ""
		}
	}
}

/// A story's minutes on one day of the allocation.
public struct CloseOutAllocationRow: Identifiable, Hashable, Sendable {
	public let story: String
	public let title: String
	public let minutes: Int
	/// Tracked time, private to this Mac.
	public let trackedMinutes: Int
	/// Whether the story has an estimate; without one it gets no minutes.
	public let hasEstimate: Bool

	public var id: String { story }
}

/// One working day of the allocation.
public struct CloseOutAllocationDay: Identifiable, Hashable, Sendable {
	public let date: String
	public let targetMinutes: Int
	public let rows: [CloseOutAllocationRow]
	public let isConfirmed: Bool
	public let isDraft: Bool

	public var id: String { date }

	public var totalMinutes: Int { rows.reduce(0) { $0 + $1.minutes } }

	/// The stories worked that day without an estimate, by number.
	public var unestimatedStories: [String] { rows.filter { !$0.hasEstimate }.map(\.story) }
}

/// A review row of an hourly or fixed-price week.
public struct CloseOutReviewRow: Identifiable, Hashable, Sendable {
	public let key: String
	public let story: String?
	public let title: String
	public let trackedMinutes: Int
	public let billedMinutes: Int
	public let overrunMinutes: Int
	public let estimateMinutes: Int?
	public let isBillable: Bool

	public var id: String { "\(key)/\(isBillable)" }
}

/// A row of the copyable table.
public struct CloseOutTableRow: Identifiable, Hashable, Sendable {
	public let key: String
	public let cells: [String]
	public let isTicked: Bool

	public var id: String { key }
}

/// The copyable table of the table step.
public struct CloseOutTable: Hashable, Sendable {
	public let sink: String
	public let title: String
	public let columns: [String]
	public let rows: [CloseOutTableRow]
	/// The text the sink takes: plain TSV, byte-exact.
	public let text: String

	public var openRows: Int { rows.count { !$0.isTicked } }
}

/// The close-out of one week or month.
public struct CloseOutSnapshot: Hashable, Sendable {
	public let periodId: String
	public let kind: CloseOutPeriodKind
	public let start: String
	public let end: String
	public let billing: CloseOutBilling
	/// `YYYY-MM-DDTHH:MM`.
	public let deadline: String
	public let days: [CloseOutDay]
	public let isTogglOn: Bool
	public let flat: [CloseOutFlatDay]
	public let unqueuedSessions: Int
	public let allocation: [CloseOutAllocationDay]
	public let review: [CloseOutReviewRow]
	public let table: CloseOutTable
	public let steps: [CloseOutStep]
	public let missing: [String]
	public let canClose: Bool
	public let isClosed: Bool
}

/// What the header's ritual button offers.
public struct CloseOutDue: Hashable, Sendable {
	public let kind: CloseOutPeriodKind
	/// A day inside the period.
	public let date: String
	/// "Week close-out".
	public let title: String
	/// `YYYY-MM-DDTHH:MM`.
	public let deadline: String
	public let isOverdue: Bool

	/// "Due Mon 08:00".
	public var timeText: String {
		let prefix = isOverdue ? "Overdue since" : "Due"
		return "\(prefix) \(CloseOutText.deadlineTitle(deadline))"
	}
}

/// A story's minutes on a day, as a draft or a confirmation sends them.
public struct CloseOutAllotment: Hashable, Sendable {
	public let story: String
	public let minutes: Int

	public init(story: String, minutes: Int) {
		self.story = story
		self.minutes = minutes
	}
}

/// Dates and hours as the close-out shows them.
public enum CloseOutText {
	/// "Mon 19" for `2026-10-19`.
	public static func dayTitle(_ day: String) -> String {
		guard let date = DayText.date(from: day) else { return day }
		return dayFormatter.string(from: date)
	}

	/// "Mon 08:00" for `2026-10-26T08:00`.
	public static func deadlineTitle(_ text: String) -> String {
		let parts = text.split(separator: "T", maxSplits: 1).map(String.init)
		guard parts.count == 2, let date = DayText.date(from: parts[0]) else { return text }
		return "\(weekdayFormatter.string(from: date)) \(parts[1])"
	}

	/// "6.5" for 390 minutes, "6" for 360.
	public static func hours(_ minutes: Int) -> String {
		let hours = Double(minutes) / 60
		return hours == hours.rounded() ? String(Int(hours)) : String(format: "%.2f", hours)
			.replacingOccurrences(of: #"0+$"#, with: "", options: .regularExpression)
	}

	private static let dayFormatter: DateFormatter = {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_GB")
		formatter.dateFormat = "EEE d"
		return formatter
	}()

	private static let weekdayFormatter: DateFormatter = {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_GB")
		formatter.dateFormat = "EEE"
		return formatter
	}()
}

extension CloseOutSnapshot {
	init(_ view: CloseOutView) {
		periodId = view.periodId
		kind = CloseOutPeriodKind(view.kind)
		start = view.start
		end = view.end
		billing = switch view.model {
		case .fte: .fte
		case .hourly: .hourly
		case .fixed: .fixed
		}
		deadline = view.deadline
		days = view.days.map { CloseOutDay(date: $0.date, kind: CloseOutDayKind($0.kind)) }
		isTogglOn = view.isTogglOn
		flat = view.flat.map { line in
			let state: CloseOutFlatDay.State = switch line.state {
			case .notQueued: .notQueued
			case .queued: .queued
			case .sent: .sent
			case .failed: .failed
			case .leave: .leave
			case .holiday: .holiday
			}
			return CloseOutFlatDay(date: line.date, state: state, seconds: Int(line.seconds))
		}
		unqueuedSessions = Int(view.unqueuedSessions)
		allocation = view.allocation.map { day in
			CloseOutAllocationDay(
				date: day.date,
				targetMinutes: Int(day.targetMinutes),
				rows: day.rows.map {
					CloseOutAllocationRow(story: $0.story, title: $0.title, minutes: Int($0.minutes), trackedMinutes: Int($0.trackedMinutes), hasEstimate: $0.hasEstimate)
				},
				isConfirmed: day.isConfirmed,
				isDraft: day.isDraft,
			)
		}
		review = view.review.map {
			CloseOutReviewRow(
				key: $0.key,
				story: $0.story,
				title: $0.title,
				trackedMinutes: Int($0.trackedMinutes),
				billedMinutes: Int($0.billedMinutes),
				overrunMinutes: Int($0.overrunMinutes),
				estimateMinutes: $0.estimateMinutes.map(Int.init),
				isBillable: $0.isBillable,
			)
		}
		table = CloseOutTable(
			sink: view.table.sink,
			title: view.table.title,
			columns: view.table.columns,
			rows: view.table.rows.map { CloseOutTableRow(key: $0.key, cells: $0.cells, isTicked: $0.isTicked) },
			text: view.table.text,
		)
		steps = view.steps.map { step in
			let kind: CloseOutStepKind = switch step.kind {
			case .toggl: .toggl
			case .allocate: .allocate
			case .review: .review
			case .table: .table
			case .close: .close
			}
			let status: CloseOutStepStatus = switch step.status {
			case .open: .open
			case .done: .done
			case .skipped: .skipped
			}
			return CloseOutStep(kind: kind, title: step.title, status: status)
		}
		missing = view.missing
		canClose = view.canClose
		isClosed = view.isClosed
	}
}

extension CloseOutDayKind {
	init(_ view: DayKindView) {
		switch view {
		case .working: self = .working
		case .leave: self = .leave
		case .holiday: self = .holiday
		case .weekend: self = .weekend
		}
	}
}
