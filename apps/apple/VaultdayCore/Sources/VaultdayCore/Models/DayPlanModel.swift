import Foundation
import Observation

/// An open task line, as the views see it.
public struct TodayTask: Identifiable, Hashable, Sendable {
	public let id: String
	public let taskId: String?
	public let text: String
	public let isInProgress: Bool
	public let depth: Int
	public let assignedTo: String?
	public let due: String?
	public let estimate: String?
}

/// One story's open work, as the views see it.
public struct TodayStory: Identifiable, Hashable, Sendable {
	public let id: String
	public let number: String?
	public let title: String
	public let status: String?
	public let nextSteps: [TodayTask]
	public let pendingInput: [TodayTask]
}

/// One line of the day log, as the views see it.
public struct TodayLogLine: Identifiable, Hashable, Sendable {
	public let id: String
	public let time: String
	public let kind: String?
	public let text: String
	public let story: String?
	public let task: String?
}

/// A busy span of the day, such as a calendar event, in minutes since midnight.
public struct BusySpan: Hashable, Sendable {
	public let startMinute: Int
	public let endMinute: Int

	public init(startMinute: Int, endMinute: Int) {
		self.startMinute = startMinute
		self.endMinute = endMinute
	}
}

/// `=` lays the list from the start of the working day; `r` re-plans from now.
public enum ScheduleMode: Sendable {
	case autoSchedule
	case replan
}

/// The note shown at the foot of the Today column when the plan changes.
public struct PlanToast: Equatable, Sendable {
	public let message: String
	/// Whether the toast offers Re-plan.
	public let offersReplan: Bool
}

/// The day's plan: the Today list read from the vault through the core in
/// the order the workspace's app state holds for the day, its totals, and
/// the edits Home makes: tick, reorder, schedule. Keeps plain Swift values
/// for the views; FFI records never leave the package.
@MainActor
@Observable
public final class DayPlanModel {
	public private(set) var workspaceName = ""
	/// The day shown, `YYYY-MM-DD`.
	public private(set) var date = ""
	public private(set) var stories: [TodayStory] = []
	public private(set) var log: [TodayLogLine] = []
	/// Task ids in the day's planned order, from the synced app state.
	public private(set) var plannedTaskIds: [String] = []
	/// The cards of the day: unfinished in list order, then the ones done on the day.
	public private(set) var items: [PlanItem] = []
	public private(set) var isToday = true
	/// Cards whose subtask list `za` folded; kept while the app runs.
	public private(set) var foldedSubtaskTaskIds: Set<String> = []
	public private(set) var totals = DayTotals()
	/// Every open task of the workspace's stories, for the sidebar's Browse row.
	public private(set) var browseCount = 0
	public private(set) var toast: PlanToast?
	public private(set) var errorMessage: String?
	/// Wall time of the last `today()` call through the bridge, in milliseconds.
	public private(set) var lastCallMilliseconds: Double?

	/// Where tasks the stored order does not name yet join the list (Settings, Rituals and routines).
	public var newTaskPosition: ListPositionSetting = .bottom
	/// Whether a day that runs over warns (Settings, Rituals and routines).
	public var overrunWarning: OverrunWarningSetting = .intoFreeTime
	/// The working day the scheduler and the Free pill use.
	public var workingHours = WorkingHoursSetting(
		kind: .fixed,
		start: DayTime(minuteOfDay: 9 * 60) ?? DayTime(minuteOfDay: 0)!,
		end: DayTime(minuteOfDay: 17 * 60 + 30) ?? DayTime(minuteOfDay: 0)!,
		days: [.monday, .tuesday, .wednesday, .thursday, .friday],
	)
	/// The calendar's busy spans of a day (`YYYY-MM-DD`), when calendars are readable.
	public var busyProvider: (String) -> [BusySpan] = { _ in [] }

	private var vault: WorkspaceHandle?
	private var state: WorkspaceStateHandle?
	/// Days whose over-free-time toast the user dismissed in this run.
	private var dismissedToastDates: Set<String> = []
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	/// `za` in Today: folds the card's subtasks, or opens them again. A card
	/// without subtasks has nothing to fold; `false` then.
	@discardableResult
	public func toggleSubtaskFold(taskId: String) -> Bool {
		guard items.first(where: { $0.taskId == taskId })?.subtasks.isEmpty == false else { return false }
		if !foldedSubtaskTaskIds.insert(taskId).inserted {
			foldedSubtaskTaskIds.remove(taskId)
		}
		return true
	}

	/// Whether the card of `taskId` shows its subtasks folded.
	public func isSubtaskListFolded(_ taskId: String) -> Bool {
		foldedSubtaskTaskIds.contains(taskId)
	}

	public var openTaskCount: Int {
		stories.reduce(0) { $0 + $1.nextSteps.count + $1.pendingInput.count }
	}

	public var unfinishedItems: [PlanItem] { items.filter { !$0.isDone } }

	/// A day before today: shown as it was, never scheduled.
	public var isPast: Bool { !date.isEmpty && date < DayText.string(now()) }

	public var doneItems: [PlanItem] { items.filter(\.isDone) }

	/// The column's title: "Thursday 15 October".
	public var title: String {
		guard let day = DayText.date(from: date) else { return date }
		return DayText.title(day)
	}

	/// Unfinished work with an estimate, in minutes.
	public var remainingMinutes: Int {
		unfinishedItems.compactMap(\.estimateMinutes).reduce(0, +)
	}

	// MARK: Loading

	/// Loads the open workspace's day `date` (`YYYY-MM-DD`; `nil` keeps the
	/// day shown, or today on the first load).
	public func load(from workspace: WorkspaceModel, date: String? = nil) {
		guard let vault = workspace.vault else {
			clear()
			return
		}
		self.vault = vault
		state = workspace.state
		load(vault, date: date ?? (self.date.isEmpty ? nil : self.date))
		reloadPlan()
	}

	/// Opens the workspace at `path` without a grant (tests, and hosts
	/// outside the sandbox) and loads the day `date`.
	public func load(workspacePath path: String, date: String? = nil) {
		do {
			let vault = try openWorkspace(path: path)
			self.vault = vault
			state = nil
			load(vault, date: date)
			reloadPlan()
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Reads the vault and the stored order again for the day shown.
	public func reload() {
		guard let vault else { return }
		load(vault, date: date)
		reloadPlan()
	}

	/// Shows the day `offset` days from the one shown.
	public func showDay(offset: Int) {
		guard let shown = DayText.date(from: date),
		      let target = Calendar.current.date(byAdding: .day, value: offset, to: shown)
		else { return }
		date = DayText.string(target)
		reload()
	}

	/// Returns to today.
	public func showToday() {
		date = DayText.string(now())
		reload()
	}

	// MARK: Editing

	/// Ticks a task or a subtask done as of today: the core changes exactly
	/// its line in the vault. `false` when the core refused.
	@discardableResult
	public func tick(taskId: String) -> Bool {
		guard let vault else { return false }
		do {
			_ = try tickTaskDone(workspace: vault, id: taskId, date: DayText.string(now()))
			errorMessage = nil
			reload()
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}

	/// Moves a task to `destination` among the unfinished ones and stores the order.
	public func move(taskId: String, to destination: Int) {
		store(order: PlanOrder.moving(taskId, to: destination, in: items))
	}

	/// Moves a task one place up (`-1`) or down (`+1`) and stores the order.
	public func step(taskId: String, by step: Int) {
		store(order: PlanOrder.stepping(taskId, by: step, in: items))
	}

	/// Runs `=` or `r` through the core's scheduler and stores the boxes it lays.
	/// A past day is refused; on a later day `r` starts where `=` does, at the start of work.
	public func schedule(_ mode: ScheduleMode) {
		guard let state else {
			errorMessage = "Scheduling needs the workspace's app state"
			return
		}
		guard !isPast else {
			errorMessage = "Past days are not planned"
			return
		}
		let mode = isToday ? mode : .autoSchedule
		let input = ScheduleDayInput(
			date: date,
			mode: mode == .autoSchedule ? .autoSchedule : .replan,
			nowMinute: UInt32(minuteOfDay(now())),
			workingStartMinute: UInt32(workingHours.start.minuteOfDay),
			workingEndMinute: UInt32(workingHours.end.minuteOfDay),
			items: items.map { item in
				ScheduleItemView(task: item.taskId, estimateMinutes: item.estimateMinutes.map(UInt32.init), isFinished: item.isDone)
			},
			busy: busyProvider(date).map(Self.busyView),
			bufferMinutes: 0,
		)
		do {
			let schedule = try state.scheduleDay(input: input)
			_ = try state.flush()
			apply(slots: schedule.boxes)
			let over = Int(schedule.overMinutes)
			dismissedToastDates.remove(date)
			toast = over > 0 && overrunWarning == .intoFreeTime
				? overToast(minutes: over)
				: PlanToast(message: scheduledMessage(mode: mode, unplaced: schedule.unplaced.count), offersReplan: false)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Hides the toast; an over-free-time toast stays hidden for the day in this run.
	public func dismissToast() {
		if toast?.offersReplan == true {
			dismissedToastDates.insert(date)
		}
		toast = nil
	}

	// MARK: Private

	private func load(_ workspace: WorkspaceHandle, date: String?) {
		do {
			let start = ContinuousClock.now
			let view = try today(workspace: workspace, date: date)
			let elapsed = ContinuousClock.now - start
			lastCallMilliseconds = Double(elapsed.components.attoseconds) / 1e15
				+ Double(elapsed.components.seconds) * 1000
			apply(view)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Builds the list of the day shown from the vault and the stored order.
	private func reloadPlan() {
		guard let vault, !date.isEmpty else { return }
		do {
			plannedTaskIds = try state?.dayOrder(date: date) ?? []
			let list = try dayList(
				workspace: vault,
				date: date,
				storedOrder: plannedTaskIds,
				placement: newTaskPosition == .top ? .top : .bottom,
				today: DayText.string(now()),
			)
			isToday = list.isToday
			items = list.items.map { PlanItem($0) }
			browseCount = Int(list.openTaskCount)
			apply(slots: try state?.timeBoxes(date: date) ?? [])
			totals = makeTotals(list)
			refreshOverToast()
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	private func makeTotals(_ list: DayListView) -> DayTotals {
		var totals = DayTotals()
		totals.plannedMinutes = Int(list.plannedMinutes)
		totals.unestimatedCount = Int(list.unestimated)
		totals.taskCount = items.count
		totals.doneCount = items.filter(\.isDone).count
		if let time = try? state?.dayTime(date: date) {
			totals.spentMinutes = Int(time.spentSeconds / 60)
			totals.loggedMinutes = Int(time.loggedSeconds / 60)
		}
		totals.freeMinutes = freeTime()
		return totals
	}

	/// The free time left in the working day shown: all of it on a later
	/// day, what remains from now today, none on a past day; `nil` on a day
	/// off, which has no working hours to measure against.
	private func freeTime() -> Int? {
		guard let shown = DayText.date(from: date), workingHours.days.contains(Self.weekday(of: shown)) else { return nil }
		let today = DayText.string(now())
		let from = date == today ? minuteOfDay(now()) : (date < today ? 24 * 60 : 0)
		let minutes = try? freeMinutes(
			date: date,
			fromMinute: UInt32(from),
			workingStartMinute: UInt32(workingHours.start.minuteOfDay),
			workingEndMinute: UInt32(workingHours.end.minuteOfDay),
			busy: busyProvider(date).map(Self.busyView),
		)
		return minutes.map(Int.init)
	}

	private func refreshOverToast() {
		let over = totals.overFreeTime(remainingMinutes: remainingMinutes)
		let shows = isToday && over > 0 && overrunWarning == .intoFreeTime && !dismissedToastDates.contains(date)
		if shows {
			toast = overToast(minutes: over)
		} else if toast?.offersReplan == true {
			toast = nil
		}
	}

	private func overToast(minutes: Int) -> PlanToast {
		PlanToast(message: "Over free time by \(DurationText.minutes(minutes))", offersReplan: true)
	}

	private func scheduledMessage(mode: ScheduleMode, unplaced: Int) -> String {
		let start = mode == .replan ? "from now" : "from \(workingHours.start.text)"
		return unplaced == 0 ? "Planned \(start); everything fits" : "Planned \(start); \(unplaced) did not fit"
	}

	private func apply(slots: [TimeBoxView]) {
		let byTask = Dictionary(slots.map { ($0.task, $0) }, uniquingKeysWith: { first, _ in first })
		items = items.map { item in
			var item = item
			item.slot = byTask[item.taskId].flatMap { box in
				DayTime(minuteOfDay: Int(box.startMinute)).map { PlanSlot(start: $0, minutes: Int(box.minutes), isLocked: box.isLocked) }
			}
			return item
		}
	}

	private func store(order: [String]) {
		guard let state else {
			errorMessage = "The order needs the workspace's app state"
			return
		}
		do {
			try state.setDayOrder(date: date, taskIds: order)
			_ = try state.flush()
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reloadPlan()
	}

	private func clear() {
		workspaceName = ""
		date = ""
		stories = []
		log = []
		plannedTaskIds = []
		items = []
		totals = DayTotals()
		browseCount = 0
		toast = nil
		vault = nil
		state = nil
		errorMessage = nil
	}

	private func apply(_ view: TodayView) {
		workspaceName = view.workspaceName
		date = view.date
		stories = view.stories.map(mapStory)
		log = view.log.enumerated().map { index, line in
			TodayLogLine(
				id: "\(index)-\(line.time)",
				time: line.time,
				kind: line.kind,
				text: line.text,
				story: line.story,
				task: line.task,
			)
		}
	}

	private func minuteOfDay(_ date: Date) -> Int {
		let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
		return (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
	}

	private static func weekday(of day: Date) -> WeekdayChoice {
		switch Calendar.current.component(.weekday, from: day) {
		case 1: .sunday
		case 2: .monday
		case 3: .tuesday
		case 4: .wednesday
		case 5: .thursday
		case 6: .friday
		default: .saturday
		}
	}

	private static func busyView(_ span: BusySpan) -> BusySpanView {
		BusySpanView(startMinute: UInt32(max(0, span.startMinute)), endMinute: UInt32(max(0, span.endMinute)))
	}
}

// MARK: - The register's edits

extension DayPlanModel: PlanEditing {
	public var shownDate: String { date }

	public var todayDate: String { DayText.string(now()) }

	public func lineState(of taskId: String) -> TaskLineState? {
		guard let vault, let task = try? taskById(workspace: vault, id: taskId) else { return nil }
		return TaskLineState(text: task.text, scheduled: task.scheduled, due: task.due, estimate: task.estimate, completion: task.completion)
	}

	public func apply(_ edit: TaskLineEdit, to taskId: String) -> LineWrite {
		guard let vault else { return .refused("No workspace is open") }
		do {
			let change = try editTask(workspace: vault, id: taskId, edit: Self.coreEdit(edit))
			return .written(old: change.oldLine ?? change.newLine, new: change.newLine)
		} catch {
			return .refused(coreErrorMessage(error))
		}
	}

	public func restore(_ taskId: String, line: String, expecting current: String) -> String? {
		guard let vault else { return "No workspace is open" }
		do {
			_ = try restoreTaskLine(workspace: vault, id: taskId, expected: current, line: line)
			return nil
		} catch {
			return coreErrorMessage(error)
		}
	}

	public func listOrder(on day: String) -> [String] {
		if day == date { return items.map(\.taskId) }
		return (try? state?.dayOrder(date: day)) ?? []
	}

	public func storeOrder(_ taskIds: [String], on day: String) -> String? {
		guard let state else { return "The order needs the workspace's app state" }
		do {
			try state.setDayOrder(date: day, taskIds: taskIds)
			_ = try state.flush()
			return nil
		} catch {
			return coreErrorMessage(error)
		}
	}

	public func refresh() {
		reload()
	}

	private static func coreEdit(_ edit: TaskLineEdit) -> TaskEditInput {
		switch edit {
		case let .done(day): .done(date: day)
		case .reopen: .reopen
		case let .schedule(day): .schedule(date: day)
		case let .due(day): .due(date: day)
		case let .estimate(estimate): .estimate(estimate: estimate)
		case let .text(text): .text(text: text)
		}
	}
}

/// Day strings and titles, in the host's calendar.
public enum DayText {
	/// `YYYY-MM-DD`.
	public static func string(_ date: Date) -> String {
		dayFormatter.string(from: date)
	}

	public static func date(from text: String) -> Date? {
		dayFormatter.date(from: text)
	}

	/// "Thursday 15 October".
	public static func title(_ date: Date) -> String {
		titleFormatter.string(from: date)
	}

	/// "Fri 16 Oct", as the quick-add confirmation's chip.
	public static func chip(_ date: Date) -> String {
		chipFormatter.string(from: date)
	}

	private static let dayFormatter = formatter("yyyy-MM-dd")
	private static let titleFormatter = formatter("EEEE d MMMM")
	private static let chipFormatter = formatter("EEE d MMM")

	private static func formatter(_ format: String) -> DateFormatter {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.calendar = Calendar(identifier: .gregorian)
		// Follows the Mac's zone as it changes, as the core does on each call.
		formatter.timeZone = .autoupdatingCurrent
		formatter.dateFormat = format
		return formatter
	}
}

private func mapStory(_ story: StoryView) -> TodayStory {
	TodayStory(
		id: story.relativePath,
		number: story.number,
		title: story.title ?? story.number ?? story.relativePath,
		status: story.status,
		nextSteps: mapTasks(story.nextSteps, in: story.relativePath, section: "next"),
		pendingInput: mapTasks(story.pendingInput, in: story.relativePath, section: "pending"),
	)
}

private func mapTasks(_ tasks: [TaskView], in path: String, section: String) -> [TodayTask] {
	tasks.enumerated().map { index, task in
		TodayTask(
			id: task.id ?? "\(path)#\(section)#\(index)",
			taskId: task.id,
			text: task.text,
			isInProgress: task.state == .inProgress,
			depth: Int(task.depth),
			assignedTo: task.assignedTo,
			due: task.due,
			estimate: task.estimate,
		)
	}
}
