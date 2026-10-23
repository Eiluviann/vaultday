import Foundation
import Observation

/// What the Today column shows: the list, or the calendar over three days or a week.
public enum CalendarSpan: String, CaseIterable, Identifiable, Sendable {
	case list = "List"
	case threeDays = "3 days"
	case week = "Week"

	public var id: String { rawValue }
}

/// One block of the day grid, as the views see it.
public struct CalendarBlock: Identifiable, Hashable, Sendable {
	public enum Kind: Hashable, Sendable {
		/// An event of one of the workspace's calendars.
		case event
		/// A pinned task, kept in the private calendar.
		case locked
		/// A task's projection from the scheduler.
		case unlocked
	}

	public let id: String
	public let kind: Kind
	public let taskId: String?
	/// The item's full name.
	public let title: String
	/// The time range and state tokens: "09:30 – 11:00 · locked · tracked 0:47".
	public let detail: String
	public let startMinute: Int
	public let endMinute: Int
	/// The tracked part of the block, 0 to 1, filled from its top.
	public let trackedShare: Double
	public let lane: Int
	public let lanes: Int
	public let isDone: Bool

	public var isTask: Bool { kind != .event }
	public var minutes: Int { max(0, endMinute - startMinute) }
}

/// One date of the grid.
public struct CalendarDay: Identifiable, Hashable, Sendable {
	/// `YYYY-MM-DD`.
	public let date: String
	public let blocks: [CalendarBlock]
	/// Now, when the date is today: the now line and the end of the past wash.
	public let nowMinute: Int?
	/// The whole date lies before today.
	public let isPast: Bool

	public var id: String { date }
	public var isToday: Bool { nowMinute != nil }

	private static let headingFormatter: DateFormatter = {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.calendar = Calendar(identifier: .gregorian)
		formatter.timeZone = .autoupdatingCurrent
		formatter.dateFormat = "EEE d"
		return formatter
	}()

	/// "Thu 15", the column head of the three-day and week grids.
	public var heading: String {
		guard let day = DayText.date(from: date) else { return date }
		return Self.headingFormatter.string(from: day)
	}
}

/// The Calendar tab and the Today column's three-day and week grids: the
/// workspace's events and the stored time boxes laid out by the core, and
/// the pin control. Unlocked blocks are the core scheduler's boxes; nothing
/// here places them. Pins go through the calendar port, which writes only
/// to the private Vaultday calendar in iCloud.
@MainActor
@Observable
public final class CalendarPanelModel {
	/// The side panel's day, the one the Today column shows.
	public private(set) var day: CalendarDay?
	/// The days of the Today column's grid when it shows three days or a week.
	public private(set) var spanDays: [CalendarDay] = []
	public private(set) var span: CalendarSpan = .list
	public private(set) var errorMessage: String?
	/// Calendar events turned into meeting sessions by the last check.
	public private(set) var recordedMeetings = 0
	/// Set when a pin covered unlocked boxes; the app re-plans the day through the core and clears it.
	public var needsReplan = false

	/// Called after a pin changed the plan, so the Today list shows it.
	public var onPlanChanged: () -> Void = {}
	/// The key cap of an action in the active scheme, from `Keymap.yaml`; messages name keys through it.
	public var keyLabel: (KeyAction) -> String? = { _ in nil }

	private var vault: WorkspaceHandle?
	private var state: WorkspaceStateHandle?
	private var calendar: any CalendarPort = NoCalendarPort()
	private var hasCalendarAccess = false
	private var date = ""
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	/// The port events are read from and pins are written to; `nil` until
	/// calendar access is allowed, when the grid shows boxes only and a pin
	/// is refused with the access message.
	public func attach(calendar: (any CalendarPort)?) {
		self.calendar = calendar ?? NoCalendarPort()
		hasCalendarAccess = calendar != nil
		reload()
	}

	/// Shows `date` (`YYYY-MM-DD`) of the open workspace.
	public func load(from workspace: WorkspaceModel, date: String) {
		vault = workspace.vault
		state = workspace.state
		self.date = date
		reload()
	}

	/// Switches the Today column between the list and the grids.
	public func show(_ span: CalendarSpan) {
		self.span = span
		reloadSpan()
	}

	/// Reads the grid again: after a sync, a plan change or a minute passing.
	public func reload() {
		guard let vault, let state, !date.isEmpty else {
			day = nil
			spanDays = []
			return
		}
		do {
			day = try loadDay(date, vault: vault, state: state)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reloadSpan()
	}

	/// The padlock or `m` / `X` on a task: pins its box to the private
	/// calendar, or unpins it. `false` when refused; `errorMessage` says why.
	@discardableResult
	public func togglePin(taskId: String, on date: String? = nil) -> Bool {
		let date = date ?? self.date
		let isToggled = edit { try $0.togglePin(workspace: $1, calendar: calendar, date: date, task: taskId) }
		if !isToggled, !hasBlock(of: taskId, on: date) {
			errorMessage = noTimeMessage()
		}
		return isToggled
	}

	/// A task dropped on the grid of `date` at `startMinute`: its box moves
	/// there and is pinned. A pin that lands on unlocked boxes asks the core
	/// to re-plan them around it.
	@discardableResult
	public func pin(taskId: String, startMinute: Int, on date: String) -> Bool {
		let floating = (spanDays + [day].compactMap(\.self))
			.filter { $0.date == date }
			.flatMap(\.blocks)
			.filter { $0.kind == .unlocked && $0.taskId != taskId }
		return edit({ try $0.pinAt(workspace: $1, calendar: calendar, date: date, task: taskId, startMinute: UInt32(max(0, startMinute))) }) { pinned in
			// Only the shown day is re-planned; the pin's real length decides what it covers.
			let start = Int(pinned.startMinute)
			let end = start + Int(pinned.minutes)
			needsReplan = date == self.date && floating.contains { $0.startMinute < end && $0.endMinute > start }
		}
	}

	/// Turns the ended events of today and yesterday into meeting sessions,
	/// once each; yesterday's catch a meeting that ended while the Mac slept
	/// past midnight.
	@discardableResult
	public func recordEndedMeetings() -> Int {
		guard hasCalendarAccess, let vault, let state else { return 0 }
		let today = now()
		let days = [Calendar.current.date(byAdding: .day, value: -1, to: today), today].compactMap(\.self).map(DayText.string)
		do {
			let count = try days.reduce(0) { total, day in
				total + Int(try state.recordEndedMeetings(workspace: vault, calendar: calendar, date: day))
			}
			if count > 0 {
				_ = try state.flush()
			}
			recordedMeetings = count
			return count
		} catch {
			errorMessage = coreErrorMessage(error)
			return 0
		}
	}

	/// The minute tick: meetings that ended become sessions, the now line moves.
	public func tick() {
		if recordEndedMeetings() > 0 {
			onPlanChanged()
		}
		reload()
	}

	public func clear() {
		vault = nil
		state = nil
		date = ""
		day = nil
		spanDays = []
		errorMessage = nil
	}

	// MARK: Private

	/// Runs one pin edit through the core; `then` sees the stored box before
	/// the plan-changed hook runs.
	private func edit(_ change: (WorkspaceStateHandle, WorkspaceHandle) throws -> TimeBoxView, then: (TimeBoxView) -> Void = { _ in }) -> Bool {
		guard let vault, let state else {
			errorMessage = "Pins need the workspace's app state"
			return false
		}
		do {
			then(try change(state, vault))
			_ = try state.flush()
			errorMessage = nil
			reload()
			onPlanChanged()
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}

	private func hasBlock(of taskId: String, on date: String) -> Bool {
		(spanDays + [day].compactMap(\.self))
			.filter { $0.date == date }
			.flatMap(\.blocks)
			.contains { $0.taskId == taskId }
	}

	/// Why a task without a block cannot be pinned, naming the auto-schedule key of the active scheme.
	private func noTimeMessage() -> String {
		let base = "This task has no time on the calendar yet; drag it onto the calendar or auto-schedule the day"
		return keyLabel(.autoSchedule).map { "\(base) with \($0)" } ?? base
	}

	private func reloadSpan() {
		guard span != .list, let vault, let state else {
			spanDays = []
			return
		}
		do {
			spanDays = try spanDates().map { try loadDay($0, vault: vault, state: state) }
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Three days from the day shown, or its Monday-to-Sunday week.
	private func spanDates() -> [String] {
		guard let shown = DayText.date(from: date) else { return [] }
		var calendar = Calendar(identifier: .gregorian)
		calendar.firstWeekday = 2
		let first: Date
		let count: Int
		switch span {
		case .list:
			return []
		case .threeDays:
			first = shown
			count = 3
		case .week:
			first = calendar.dateInterval(of: .weekOfYear, for: shown)?.start ?? shown
			count = 7
		}
		return (0 ..< count).compactMap { calendar.date(byAdding: .day, value: $0, to: first).map(DayText.string) }
	}

	private func loadDay(_ date: String, vault: WorkspaceHandle, state: WorkspaceStateHandle) throws -> CalendarDay {
		CalendarDay(try state.calendarDay(workspace: vault, calendar: calendar, date: date))
	}
}

/// The port while calendar access is not allowed: no events, and every
/// write refused with the access message.
final class NoCalendarPort: CalendarPort, @unchecked Sendable {
	func events(from _: String, to _: String) throws -> [CalendarEvent] { [] }

	func writeTimeBox(calendar _: String, title _: String, timeBox _: CalendarTimeBox) throws {
		throw CalendarFailure.Unavailable(reason: CalendarRefusal.accessNotGranted.localizedDescription)
	}

	func deleteTimeBox(calendar _: String, id _: String) throws {
		throw CalendarFailure.Unavailable(reason: CalendarRefusal.accessNotGranted.localizedDescription)
	}
}

extension CalendarDay {
	init(_ view: CalendarDayView) {
		self.init(
			date: view.date,
			blocks: view.blocks.map(CalendarBlock.init),
			nowMinute: view.nowMinute.map(Int.init),
			isPast: view.isPast,
		)
	}
}

extension CalendarBlock {
	init(_ view: CalendarBlockView) {
		let kind: Kind = switch view.kind {
		case .event: .event
		case .lockedTask: .locked
		case .unlockedTask: .unlocked
		}
		self.init(
			id: view.id,
			kind: kind,
			taskId: view.task,
			title: view.title,
			detail: view.detail,
			startMinute: Int(view.startMinute),
			endMinute: Int(view.endMinute),
			trackedShare: Double(view.trackedPermille) / 1000,
			lane: Int(view.lane),
			lanes: max(1, Int(view.lanes)),
			isDone: view.isDone,
		)
	}
}
