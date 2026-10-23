import Foundation
import XCTest
@testable import VaultdayCore

/// The side panel's models over the mock vault and a fake event store:
/// blocks placed from the core scheduler's output, pins written only to the
/// private calendar in iCloud, the Reminders intake's import-then-delete
/// order, and the Hours maths.
@MainActor
final class SidePanelModelTests: XCTestCase {
	/// Friday 23 October 2026, 08:00 in the host's time zone: before work, so `=` lays the whole day.
	private static let friday: Date = {
		var parts = DateComponents()
		parts.year = 2026
		parts.month = 10
		parts.day = 23
		parts.hour = 8
		return Calendar.current.date(from: parts)!
	}()

	private static let day = "2026-10-23"
	private let iCloud = CalendarSourceInfo(id: "src-icloud", title: "iCloud", kind: .iCloud)
	private let exchange = CalendarSourceInfo(id: "src-exchange", title: "Work Exchange", kind: .exchange)

	private func openWorkspace(_ harness: OnboardingHarness) throws -> WorkspaceModel {
		let workspace = WorkspaceModel(session: harness.session)
		workspace.grantFolder(try TestFixtures.copyOfMockVault(self))
		XCTAssertEqual(workspace.phase, .open)
		return workspace
	}

	/// An event of the Exchange "Work" calendar on the test day, in local time.
	private func workEvent(_ id: String, _ title: String, from: Int, to: Int, on day: Date = friday, meetingKey: String? = nil, isAttended: Bool = true) -> StoredEvent {
		let midnight = Calendar.current.startOfDay(for: day)
		return StoredEvent(
			id: id,
			calendarId: "ex-work",
			title: title,
			start: midnight.addingTimeInterval(TimeInterval(from * 60)),
			end: midnight.addingTimeInterval(TimeInterval(to * 60)),
			isAllDay: false,
			url: nil,
			meetingKey: meetingKey ?? "uid-\(id)",
			isAttended: isAttended,
		)
	}

	/// A store with the Exchange work calendar, picked, and a decoy Exchange
	/// calendar named Vaultday that must never be written.
	private func store(sources: [CalendarSourceInfo]) -> FakeEventStore {
		let store = FakeEventStore(sources: sources)
		store.calendarList = [
			CalendarInfo(id: "ex-work", title: "Work", source: exchange, allowsModifications: true),
			CalendarInfo(id: "ex-decoy", title: "Vaultday", source: exchange, allowsModifications: true),
		]
		store.events = ["standup": workEvent("standup", "Team stand-up", from: 9 * 60, to: 10 * 60)]
		return store
	}

	private struct Home {
		let workspace: WorkspaceModel
		let plan: DayPlanModel
		let panel: CalendarPanelModel
		let source: EventKitCalendarSource
	}

	/// The open workspace with its day planned by the core's `=` around the picked calendar.
	private func plannedHome(store: FakeEventStore) throws -> Home {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		workspace.setCalendar(id: "ex-work", title: "Work", isPicked: true)
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: workspace.pickedCalendarIds)
		let plan = DayPlanModel(now: { Self.friday })
		plan.busyProvider = { source.busySpans(on: $0) }
		plan.load(from: workspace, date: Self.day)
		plan.schedule(.autoSchedule)
		XCTAssertNil(plan.errorMessage)
		let panel = CalendarPanelModel(now: { Self.friday })
		panel.onPlanChanged = { plan.reload() }
		panel.attach(calendar: source)
		panel.load(from: workspace, date: Self.day)
		return Home(workspace: workspace, plan: plan, panel: panel, source: source)
	}

	// MARK: Calendar

	func testUnlockedBlocksArePlacedWhereTheCoreSchedulerLaidThem() throws {
		let home = try plannedHome(store: store(sources: [iCloud, exchange]))
		let day = try XCTUnwrap(home.panel.day)
		let event = try XCTUnwrap(day.blocks.first { $0.kind == .event })
		XCTAssertEqual(event.title, "Team stand-up")
		XCTAssertEqual(event.detail, "09:00 – 10:00 · Work")

		let slotted = home.plan.items.filter { $0.slot != nil }
		XCTAssertFalse(slotted.isEmpty, "the scheduler laid at least one task")
		for item in slotted {
			let block = try XCTUnwrap(day.blocks.first { $0.taskId == item.taskId }, item.taskId)
			XCTAssertEqual(block.kind, .unlocked)
			XCTAssertEqual(block.startMinute, item.slot?.start.minuteOfDay, "placed by the core, not computed here")
			XCTAssertEqual(block.minutes, item.slot?.minutes)
			XCTAssertTrue(block.detail.contains("unlocked"), block.detail)
			XCTAssertEqual(block.title, item.text, "the title is the task's full name")
		}
		let wired = try XCTUnwrap(day.blocks.first { $0.taskId == "task-2" })
		XCTAssertEqual(wired.startMinute, 10 * 60, "the 2h task waits for the stand-up to end")
	}

	func testAPinIsWrittenOnlyToTheVaultdayCalendarInICloudAndTheExchangeCalendarIsUntouched() throws {
		let store = store(sources: [iCloud, exchange])
		let exchangeBefore = store.events
		let home = try plannedHome(store: store)

		XCTAssertTrue(home.panel.togglePin(taskId: "task-2"), home.panel.errorMessage ?? "")
		let written = store.events.values.filter { !exchangeBefore.keys.contains($0.id) }
		XCTAssertEqual(written.count, 1)
		let privateCalendar = try XCTUnwrap(store.calendarList.first { $0.id == written.first?.calendarId })
		XCTAssertEqual(privateCalendar.title, "Vaultday")
		XCTAssertEqual(privateCalendar.source, iCloud, "the time box lands in the iCloud calendar")
		XCTAssertEqual(written.first?.title, "Wire the payment form to the gateway sandbox")
		XCTAssertTrue(written.first?.url?.absoluteString.hasPrefix("vaultday://time-box/") == true)
		XCTAssertFalse(store.events.values.contains { $0.calendarId == "ex-decoy" }, "the Exchange calendar named Vaultday is never written")
		XCTAssertEqual(store.events.filter { exchangeBefore.keys.contains($0.key) }, exchangeBefore, "Exchange events are untouched")
		XCTAssertEqual(home.panel.day?.blocks.first { $0.taskId == "task-2" }?.kind, .locked)
		XCTAssertEqual(home.plan.items.first { $0.taskId == "task-2" }?.slot?.isLocked, true, "the card shows the pin after a reload")

		XCTAssertTrue(home.panel.togglePin(taskId: "task-2"))
		XCTAssertEqual(store.events, exchangeBefore, "unpinning deletes the private event again")
		XCTAssertEqual(home.panel.day?.blocks.first { $0.taskId == "task-2" }?.kind, .unlocked)
	}

	func testWithICloudCalendarOffAPinIsRefusedWithTheDecidedMessage() throws {
		let store = store(sources: [exchange])
		let exchangeBefore = store.events
		let home = try plannedHome(store: store)

		XCTAssertFalse(home.panel.togglePin(taskId: "task-2"))
		XCTAssertEqual(home.panel.errorMessage, CalendarRefusal.iCloudCalendarOff.localizedDescription)
		XCTAssertEqual(store.events, exchangeBefore)
		XCTAssertEqual(store.created, [], "no calendar is made outside iCloud")
		XCTAssertEqual(home.panel.day?.blocks.first { $0.taskId == "task-2" }?.kind, .unlocked, "a refused write leaves the box floating")
	}

	func testDroppingATaskOnTheGridPinsItThereAndAsksForAReplanWhenItCoversFloatingWork() throws {
		let store = store(sources: [iCloud, exchange])
		let home = try plannedHome(store: store)
		let floating = try XCTUnwrap(home.panel.day?.blocks.first { $0.kind == .unlocked && $0.taskId != "task-5" })
		var flagWhenTheHookRan: Bool?
		home.panel.onPlanChanged = { [panel = home.panel] in flagWhenTheHookRan = panel.needsReplan }

		XCTAssertTrue(home.panel.pin(taskId: "task-5", startMinute: floating.startMinute + 30, on: Self.day))
		let pinned = try XCTUnwrap(home.panel.day?.blocks.first { $0.taskId == "task-5" })
		XCTAssertEqual(pinned.kind, .locked)
		XCTAssertEqual(pinned.startMinute, floating.startMinute + 30)
		XCTAssertEqual(flagWhenTheHookRan, true, "the app sees the re-plan request when the plan changes")

		XCTAssertFalse(home.panel.pin(taskId: "task-2", startMinute: floating.startMinute, on: Self.day), "two pins never share time")
		XCTAssertEqual(home.panel.errorMessage, "Another locked block already holds that time")
	}

	func testThePadlockOfAnotherDaysBlockActsOnThatDayAndAsksNoReplan() throws {
		let store = store(sources: [iCloud, exchange])
		let home = try plannedHome(store: store)
		home.panel.show(.week)
		XCTAssertTrue(home.panel.pin(taskId: "task-5", startMinute: 10 * 60, on: "2026-10-24"))
		XCTAssertFalse(home.panel.needsReplan, "only the shown day is re-planned")
		let saturday = { home.panel.spanDays.first { $0.date == "2026-10-24" }?.blocks.first { $0.taskId == "task-5" } }
		XCTAssertEqual(saturday()?.kind, .locked)

		XCTAssertTrue(home.panel.togglePin(taskId: "task-5", on: "2026-10-24"), home.panel.errorMessage ?? "")
		XCTAssertEqual(saturday()?.kind, .unlocked)

		XCTAssertTrue(home.panel.pin(taskId: "task-5", startMinute: 15 * 60, on: "2026-10-25"))
		XCTAssertNil(saturday(), "dragged to Sunday, the box leaves Saturday")
		XCTAssertEqual(store.events.values.filter { $0.url?.absoluteString.hasPrefix("vaultday://") == true }.count, 1, "one event, moved")
	}

	func testPinningATaskWithoutTimeNamesTheAutoScheduleKeyOfTheActiveScheme() throws {
		let home = try plannedHome(store: store(sources: [iCloud, exchange]))
		home.panel.keyLabel = { $0 == .autoSchedule ? "⇧⌘R" : nil }
		XCTAssertFalse(home.panel.togglePin(taskId: "task-without-time"))
		XCTAssertEqual(home.panel.errorMessage, "This task has no time on the calendar yet; drag it onto the calendar or auto-schedule the day with ⇧⌘R")
	}

	func testWithoutCalendarAccessTheGridStillShowsBoxesAndAPinSaysWhy() throws {
		let home = try plannedHome(store: store(sources: [iCloud, exchange]))
		home.panel.attach(calendar: nil)
		XCTAssertFalse(home.panel.day?.blocks.contains { $0.kind == .event } ?? true, "no access, no events")
		XCTAssertTrue(home.panel.day?.blocks.contains { $0.kind == .unlocked } ?? false)
		XCTAssertFalse(home.panel.togglePin(taskId: "task-2"))
		XCTAssertEqual(home.panel.errorMessage, CalendarRefusal.accessNotGranted.localizedDescription)
	}

	func testTheWeekAndThreeDayGridsCoverTheirDates() throws {
		let home = try plannedHome(store: store(sources: [iCloud, exchange]))
		home.panel.show(.threeDays)
		XCTAssertEqual(home.panel.spanDays.map(\.date), ["2026-10-23", "2026-10-24", "2026-10-25"])
		home.panel.show(.week)
		XCTAssertEqual(home.panel.spanDays.map(\.date), (19 ... 25).map { "2026-10-\($0)" }, "Monday to Sunday")
		XCTAssertEqual(home.panel.spanDays.first?.heading, "Mon 19")
		home.panel.show(.list)
		XCTAssertEqual(home.panel.spanDays, [])
	}

	func testEndedEventsBecomeMeetingSessionsOnceAndCountInHours() throws {
		// The meetings sit on the day before the injected clock, which the
		// panel records alongside today: they have ended whatever the hour.
		let now = Date()
		let day = try XCTUnwrap(Calendar.current.date(byAdding: .day, value: -1, to: now))
		let minutesToday = 14 * 60
		let store = FakeEventStore(sources: [iCloud, exchange])
		store.calendarList = [CalendarInfo(id: "ex-work", title: "Work", source: exchange, allowsModifications: true)]
		store.events = ["sync": workEvent("sync", "WEB-142 sync", from: minutesToday - 65, to: minutesToday - 5, on: day)]
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		workspace.setCalendar(id: "ex-work", title: "Work", isPicked: true)
		let panel = CalendarPanelModel(now: { now })
		panel.attach(calendar: EventKitCalendarSource(store: store, pickedCalendarIds: workspace.pickedCalendarIds))
		panel.load(from: workspace, date: DayText.string(day))

		XCTAssertEqual(panel.recordEndedMeetings(), 1)
		XCTAssertEqual(panel.recordEndedMeetings(), 0, "a meeting is recorded once")

		let hours = HoursModel()
		hours.load(from: workspace, date: DayText.string(day))
		let row = try XCTUnwrap(hours.rows.first { $0.number == "WEB-142" })
		XCTAssertEqual(row.trackedMinutes, 60, "the ticket number in the title files the meeting under its story")
		XCTAssertEqual(hours.trackedMinutes, 60)
	}

	func testDeclinedAndFreeEventsAreNoMeetingAndOverlapsCountOnce() throws {
		// The meetings sit on the day before the injected clock, which the
		// panel records alongside today: they have ended whatever the hour.
		let now = Date()
		let day = try XCTUnwrap(Calendar.current.date(byAdding: .day, value: -1, to: now))
		let minutesToday = 14 * 60
		let store = FakeEventStore(sources: [iCloud, exchange])
		store.calendarList = [CalendarInfo(id: "ex-work", title: "Work", source: exchange, allowsModifications: true)]
		store.events = [
			"declined": workEvent("declined", "WEB-142 planning", from: minutesToday - 125, to: minutesToday - 65, on: day, isAttended: false),
			"sync": workEvent("sync", "WEB-142 sync", from: minutesToday - 125, to: minutesToday - 65, on: day),
			"review": workEvent("review", "WEB-142 review", from: minutesToday - 95, to: minutesToday - 35, on: day),
		]
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		workspace.setCalendar(id: "ex-work", title: "Work", isPicked: true)
		let panel = CalendarPanelModel(now: { now })
		panel.attach(calendar: EventKitCalendarSource(store: store, pickedCalendarIds: workspace.pickedCalendarIds))
		panel.load(from: workspace, date: DayText.string(day))

		XCTAssertEqual(panel.recordEndedMeetings(), 2, "the declined invitation is no meeting")
		let hours = HoursModel()
		hours.load(from: workspace, date: DayText.string(day))
		XCTAssertEqual(hours.trackedMinutes, 90, "the half hour both meetings share counts once")
	}

	func testARenamedAndMovedMeetingKeepsItsOneSession() throws {
		// The meetings sit on the day before the injected clock, which the
		// panel records alongside today: they have ended whatever the hour.
		let now = Date()
		let day = try XCTUnwrap(Calendar.current.date(byAdding: .day, value: -1, to: now))
		let minutesToday = 14 * 60
		let store = FakeEventStore(sources: [iCloud, exchange])
		store.calendarList = [CalendarInfo(id: "ex-work", title: "Work", source: exchange, allowsModifications: true)]
		store.events = ["review": workEvent("review", "Design review", from: minutesToday - 125, to: minutesToday - 65, on: day)]
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		workspace.setCalendar(id: "ex-work", title: "Work", isPicked: true)
		let panel = CalendarPanelModel(now: { now })
		panel.attach(calendar: EventKitCalendarSource(store: store, pickedCalendarIds: workspace.pickedCalendarIds))
		panel.load(from: workspace, date: DayText.string(day))
		XCTAssertEqual(panel.recordEndedMeetings(), 1)

		store.events = ["review": workEvent("review", "Design review, moved", from: minutesToday - 70, to: minutesToday - 10, on: day)]
		XCTAssertEqual(panel.recordEndedMeetings(), 0, "the same store key is the same meeting")
		XCTAssertEqual(workspace.state?.sessions().count, 1)
	}

	// MARK: Hours

	func testHoursShowTrackedEstimateAndRemainingPerStoryAgainstTheDailyTarget() throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let hours = HoursModel()
		hours.load(from: workspace, date: Self.day)
		XCTAssertNil(hours.errorMessage)
		XCTAssertEqual(hours.targetMinutes, 8 * 60, "hours_per_day 8.0")
		XCTAssertEqual(hours.targetText, "0m of 8h")
		XCTAssertEqual(hours.rows.map(\.number), ["WEB-142", "WEB-157"], "done stories and ones without budget or time are left out")
		let checkout = try XCTUnwrap(hours.rows.first)
		XCTAssertEqual(checkout.estimateMinutes, 3 * 8 * 60, "3d is three working days")
		XCTAssertEqual(checkout.remainingMinutes, 3 * 8 * 60)
		XCTAssertEqual(checkout.budgetText, "Estimate 24h · 24h left")
	}

	func testHoursRowsSayWhatIsLeftOrOverAndWhereTheBudgetCameFrom() {
		let over = HoursRow(number: "WEB-1", title: "Over", trackedMinutes: 75, estimateMinutes: 60, remainingMinutes: 0, overtimeMinutes: 15, isEstimateFromTasks: true)
		XCTAssertTrue(over.isOver)
		XCTAssertEqual(over.share, 1)
		XCTAssertEqual(over.budgetText, "Estimate 1h from tasks · 15m over")
		let open = HoursRow(number: "WEB-2", title: "Open", trackedMinutes: 30, estimateMinutes: 120, remainingMinutes: 90, overtimeMinutes: 0, isEstimateFromTasks: false)
		XCTAssertEqual(open.share, 0.25)
		XCTAssertEqual(open.budgetText, "Estimate 2h · 1h 30m left")
		let none = HoursRow(number: "WEB-3", title: "None", trackedMinutes: 10, estimateMinutes: nil, remainingMinutes: 0, overtimeMinutes: 0, isEstimateFromTasks: false)
		XCTAssertNil(none.share)
		XCTAssertEqual(none.budgetText, "No estimate")
	}

	// MARK: Reminders intake

	func testEachReminderIsDeletedOnlyAfterItsLineIsWritten() async throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let root = URL(fileURLWithPath: workspace.rootPath)
		workspace.setIntakeList(id: "list-web", title: "Website", isPicked: true, story: "WEB-157")
		workspace.setIntakeList(id: "list-catch", title: "Inbox", isPicked: true)
		XCTAssertEqual(workspace.intakeLists.map(\.story), [nil, "WEB-157"], "Inbox, then Website")

		let reminders = FakeReminderStore()
		reminders.items = [
			"list-web": [ReminderItem(id: "r1", externalId: "ext-1", title: "Ask Lee about the provider", dueDate: "2026-10-28")],
			"list-catch": [
				ReminderItem(id: "r2", externalId: "ext-2", title: "Call the bank", dueDate: "2026-10-24", hasDueTime: true),
				ReminderItem(id: "r3", externalId: "ext-3", title: "Water the plants", isRecurring: true),
			],
		]
		reminders.beforeRemove = { id in
			let externalId = reminders.allItems.first { $0.id == id }?.externalId ?? "?"
			return Self.vaultText(root).contains("[reminder:: \(externalId)]")
		}
		let intake = RemindersIntakeModel(store: reminders, now: { Self.friday })

		let runResult = await intake.run(workspace: workspace)
		let run = try XCTUnwrap(runResult)
		XCTAssertEqual(run.filed, 2)
		XCTAssertEqual(run.left, ["Water the plants: Repeats in Reminders; repeat rules are not imported yet"])
		XCTAssertEqual(reminders.removed.map(\.id).sorted(), ["r1", "r2"])
		XCTAssertTrue(reminders.removed.allSatisfy(\.lineWasWritten), "no item is deleted before its line exists")
		XCTAssertEqual(intake.message, "Filed 2 reminders · Left in Reminders: Water the plants")
		XCTAssertEqual(run.leftTitles, ["Water the plants"])
		let catchAll = try String(contentsOf: root.appending(path: "Inbox/Reminders.md"), encoding: .utf8)
		XCTAssertTrue(catchAll.contains("Call the bank [id:: "))
		XCTAssertTrue(catchAll.contains("[scheduled:: 2026-10-24] [reminder:: ext-2]"))
	}

	func testAnInterruptedImportIsFinishedWithoutASecondLine() async throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let root = URL(fileURLWithPath: workspace.rootPath)
		workspace.setIntakeList(id: "list-catch", title: "Inbox", isPicked: true)
		let reminders = FakeReminderStore()
		reminders.items = ["list-catch": [ReminderItem(id: "r1", externalId: "ext-1", title: "Renew the passport")]]
		reminders.failRemovals = true
		let intake = RemindersIntakeModel(store: reminders, now: { Self.friday })

		let interruptedResult = await intake.run(workspace: workspace)
		let interrupted = try XCTUnwrap(interruptedResult)
		XCTAssertEqual(interrupted.filed, 0)
		XCTAssertEqual(interrupted.failed.count, 1, "written, but Reminders kept the item")
		XCTAssertEqual(reminders.items["list-catch"]?.count, 1)

		reminders.failRemovals = false
		let finishedResult = await intake.run(workspace: workspace)
		let finished = try XCTUnwrap(finishedResult)
		XCTAssertEqual(finished.filed, 1)
		XCTAssertEqual(Self.vaultText(root).components(separatedBy: "[reminder:: ext-1]").count - 1, 1, "one line, not two")
	}

	func testAnItemThatCannotBeFiledStaysInReminders() async throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		workspace.setIntakeList(id: "list-gone", title: "Old project", isPicked: true, story: "WEB-999")
		let reminders = FakeReminderStore()
		reminders.items = ["list-gone": [ReminderItem(id: "r1", externalId: "ext-1", title: "Orphan")]]
		let intake = RemindersIntakeModel(store: reminders, now: { Self.friday })

		let runResult = await intake.run(workspace: workspace)
		let run = try XCTUnwrap(runResult)
		XCTAssertEqual(run.filed, 0)
		XCTAssertEqual(run.failed.count, 1)
		XCTAssertEqual(reminders.removed.count, 0)

		workspace.setIntakeList(id: "list-gone", title: "Old project", isPicked: false)
		XCTAssertEqual(workspace.intakeLists, [])
		let emptyRun = await intake.run(workspace: workspace)
		XCTAssertNil(emptyRun, "no intake lists, no run")
	}

	func testWithoutRemindersAccessNothingRuns() async throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		workspace.setIntakeList(id: "list-catch", title: "Inbox", isPicked: true)
		let reminders = FakeReminderStore()
		reminders.access = .denied
		let intake = RemindersIntakeModel(store: reminders)
		let deniedRun = await intake.run(workspace: workspace)
		XCTAssertNil(deniedRun)
		await intake.requestAccess()
		XCTAssertEqual(intake.message, RemindersIntakeModel.accessMessage)
		XCTAssertEqual(intake.lists, [])
	}

	// MARK: Grid and keys

	func testTheGridCoversTheWorkingDayAndEveryBlockAndSnapsDrops() {
		let block = CalendarBlock(id: "late", kind: .event, taskId: nil, title: "Late call", detail: "", startMinute: 19 * 60, endMinute: 20 * 60 + 30, trackedShare: 0, lane: 0, lanes: 1, isDone: false)
		let day = CalendarDay(date: Self.day, blocks: [block], nowMinute: nil, isPast: false)
		let scale = CalendarGridScale.fitting(days: [day], workingStart: 9 * 60, workingEnd: 17 * 60 + 30)
		XCTAssertEqual(scale.firstHour, 9)
		XCTAssertEqual(scale.endHour, 21, "widened to the late call")
		XCTAssertEqual(CalendarGridScale.fitting(days: [], workingStart: 10 * 60, workingEnd: 12 * 60).hours.count, 8, "at least eight hours")
		XCTAssertEqual(scale.y(forMinute: 15 * 60, height: 1200), 600)
		XCTAssertEqual(scale.minute(atY: 607, height: 1200), 15 * 60, "snapped to a quarter hour")
		XCTAssertEqual(scale.minute(atY: 5000, height: 1200), 20 * 60 + 45, "kept inside the grid")
		XCTAssertEqual(CalendarGridScale.label(hour: 8), "08:00")
	}

	func testMPinsUnderVimMotionsAndTheCapShowsM() throws {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		let dispatcher = KeyDispatcher(keymap: keymap, scheme: .vim)
		XCTAssertEqual(dispatcher.handle(KeyStroke(key: "m")), .action(.togglePin))
		XCTAssertEqual(dispatcher.handle(KeyStroke(key: "x", modifiers: .shift)), .unbound, "the extra X binding is gone; lock stays on m")
		XCTAssertEqual(dispatcher.handle(KeyStroke(key: "x")), .action(.completeTask), "bare x still ticks")
		XCTAssertEqual(dispatcher.label(for: .togglePin), "m")
		dispatcher.setScheme(.standard)
		// ⌘L is the design note's Day log key, so the padlock moved to ⌥⌘L.
		XCTAssertEqual(dispatcher.handle(KeyStroke(key: "l", modifiers: [.option, .command])), .action(.togglePin))
		XCTAssertEqual(dispatcher.handle(KeyStroke(key: "l", modifiers: .command)), .action(.appendDayLog))
	}

	// MARK: Helpers

	/// Every note of the vault copy, joined.
	private nonisolated static func vaultText(_ root: URL) -> String {
		let files = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)?
			.compactMap { $0 as? URL }
			.filter { $0.pathExtension == "md" } ?? []
		return files.compactMap { try? String(contentsOf: $0, encoding: .utf8) }.joined(separator: "\n")
	}
}

/// Reminders in memory. `beforeRemove` reports, for each removal, whether
/// the item's line was already in the vault at that moment.
final class FakeReminderStore: ReminderStoreAccess, @unchecked Sendable {
	struct Removal: Equatable {
		let id: String
		let lineWasWritten: Bool
	}

	var access: CalendarAccess = .fullAccess
	var items: [String: [ReminderItem]] = [:]
	var removed: [Removal] = []
	var failRemovals = false
	var beforeRemove: (String) -> Bool = { _ in true }

	var allItems: [ReminderItem] { items.values.flatMap(\.self) }

	func requestFullAccess() async throws -> Bool { access == .fullAccess }

	func lists() -> [ReminderListInfo] {
		items.keys.sorted().map { ReminderListInfo(id: $0, title: $0, sourceTitle: "iCloud") }
	}

	func incompleteReminders(inList listId: String) async -> [ReminderItem] {
		(items[listId] ?? []).filter { !$0.isCompleted }
	}

	func removeReminder(id: String) throws {
		if failRemovals {
			throw NSError(domain: "FakeReminderStore", code: 1, userInfo: [NSLocalizedDescriptionKey: "Reminders is busy"])
		}
		removed.append(Removal(id: id, lineWasWritten: beforeRemove(id)))
		for key in items.keys {
			items[key]?.removeAll { $0.id == id }
		}
	}
}
