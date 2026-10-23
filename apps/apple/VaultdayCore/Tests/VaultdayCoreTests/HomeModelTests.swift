import Foundation
import XCTest
@testable import VaultdayCore

/// Home's models over the mock vault: the Today list's order and where it is
/// stored, a tick changing one line, the quick-add confirmation, the All
/// view's union, and the rituals' next times.
@MainActor
final class HomeModelTests: XCTestCase {
	/// Friday 23 October 2026, 13:00 in the host's time zone.
	private static let friday: Date = {
		var parts = DateComponents()
		parts.year = 2026
		parts.month = 10
		parts.day = 23
		parts.hour = 13
		return Calendar.current.date(from: parts)!
	}()

	private static let storyNote = "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md"

	private func openWorkspace(_ harness: OnboardingHarness, vault: URL) -> WorkspaceModel {
		let workspace = WorkspaceModel(session: harness.session)
		workspace.grantFolder(vault)
		XCTAssertEqual(workspace.phase, .open)
		return workspace
	}

	private func plan(_ workspace: WorkspaceModel, position: ListPositionSetting = .bottom) -> DayPlanModel {
		let plan = DayPlanModel(now: { Self.friday })
		plan.newTaskPosition = position
		plan.load(from: workspace, date: "2026-10-23")
		return plan
	}

	@discardableResult
	private func quickAdd(_ line: String, in workspace: WorkspaceModel) -> QuickAddEntry? {
		let model = QuickAddModel(now: { Self.friday })
		model.attach(to: workspace)
		model.text = line
		model.submit()
		let entry = model.entry
		XCTAssertTrue(model.save(), model.message ?? "not saved")
		return entry
	}

	// MARK: Today list ordering

	func testTheTodayListKeepsItsStoredOrderAcrossARelaunchAndNewTasksJoinWhereTheSettingSays() throws {
		let harness = try OnboardingHarness(self)
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspace = openWorkspace(harness, vault: vault)

		let first = try XCTUnwrap(quickAdd("Draft the coupon copy today 30m WEB-142", in: workspace))
		let second = try XCTUnwrap(quickAdd("Ask about the signup form today 15m WEB-157", in: workspace))
		let today = plan(workspace)
		XCTAssertEqual(today.title, "Friday 23 October")
		XCTAssertTrue(today.isToday)
		XCTAssertEqual(today.items.map(\.taskId), ["task-2", first.id, second.id], "the overdue task, then the vault's order")

		today.move(taskId: second.id, to: 0)
		XCTAssertEqual(today.items.map(\.taskId), [second.id, "task-2", first.id])
		today.step(taskId: "task-2", by: 1)
		XCTAssertEqual(today.items.map(\.taskId), [second.id, first.id, "task-2"])
		XCTAssertEqual(today.plannedTaskIds, [second.id, first.id, "task-2"])

		let relaunched = try harness.relaunched()
		let reopened = WorkspaceModel(session: relaunched.session)
		reopened.restoreAtLaunch()
		XCTAssertEqual(plan(reopened).items.map(\.taskId), [second.id, first.id, "task-2"], "the order lives in the workspace's app state")

		let third = try XCTUnwrap(quickAdd("Check the tax rounding today WEB-142", in: reopened))
		XCTAssertEqual(plan(reopened, position: .top).items.first?.taskId, third.id)
		XCTAssertEqual(plan(reopened, position: .bottom).items.last?.taskId, third.id)
	}

	func testZaFoldsTheSelectedCardsSubtasksAndOnlyACardThatHasThem() throws {
		let harness = try OnboardingHarness(self)
		let workspace = openWorkspace(harness, vault: try TestFixtures.copyOfMockVault(self))
		let first = try XCTUnwrap(quickAdd("Draft the coupon copy today 30m WEB-142", in: workspace))
		let today = plan(workspace)
		let withSubtasks = try XCTUnwrap(today.items.first { !$0.subtasks.isEmpty }, "task-2 has subtasks")
		XCTAssertFalse(today.isSubtaskListFolded(withSubtasks.taskId))
		XCTAssertTrue(today.toggleSubtaskFold(taskId: withSubtasks.taskId))
		XCTAssertTrue(today.isSubtaskListFolded(withSubtasks.taskId))
		XCTAssertTrue(today.toggleSubtaskFold(taskId: withSubtasks.taskId))
		XCTAssertFalse(today.isSubtaskListFolded(withSubtasks.taskId), "za again opens them")
		XCTAssertFalse(today.toggleSubtaskFold(taskId: first.id), "a card without subtasks has nothing to fold")
		XCTAssertFalse(today.isSubtaskListFolded(first.id))
	}

	func testTheOrderingRulesMoveOnlyUnfinishedTasks() {
		let items = ["a", "b", "c"].map { item($0, state: .open) } + [item("d", state: .done)]
		XCTAssertEqual(PlanOrder.moving("c", to: 0, in: items), ["c", "a", "b", "d"])
		XCTAssertEqual(PlanOrder.moving("a", to: 9, in: items), ["b", "c", "a", "d"])
		XCTAssertEqual(PlanOrder.moving("d", to: 0, in: items), ["a", "b", "c", "d"], "a done task stays below")
		XCTAssertEqual(PlanOrder.stepping("a", by: -1, in: items), ["a", "b", "c", "d"])
		XCTAssertEqual(PlanOrder.stepping("b", by: 1, in: items), ["a", "c", "b", "d"])
		XCTAssertEqual(PlanOrder.destination(forDropBefore: 3, from: 0), 2)
		XCTAssertEqual(PlanOrder.destination(forDropBefore: 0, from: 2), 0)
	}

	// MARK: Ticking

	func testTickingATaskChangesExactlyOneVaultLineAndKeepsItOnTheDay() throws {
		let harness = try OnboardingHarness(self)
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspace = openWorkspace(harness, vault: vault)
		let today = plan(workspace)
		let note = vault.appending(path: Self.storyNote)
		let before = try String(contentsOf: note, encoding: .utf8).components(separatedBy: "\n")

		XCTAssertTrue(today.tick(taskId: "task-2"))

		let after = try String(contentsOf: note, encoding: .utf8).components(separatedBy: "\n")
		XCTAssertEqual(after.count, before.count)
		let changed = zip(before, after).filter { $0 != $1 }
		XCTAssertEqual(changed.count, 1)
		XCTAssertTrue(changed.first?.1.hasPrefix("- [x] Wire the payment form") == true, changed.first?.1 ?? "")
		XCTAssertTrue(changed.first?.1.contains("[completion:: 2026-10-23]") == true)
		XCTAssertEqual(today.doneItems.map(\.taskId), ["task-2"])
		XCTAssertEqual(today.totals.doneCount, 1)
	}

	// MARK: The register through the core

	func testCutPasteAndUndoWriteThroughTheCoreAndPutTheVaultBack() throws {
		let harness = try OnboardingHarness(self)
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspace = openWorkspace(harness, vault: vault)
		let today = plan(workspace)
		let register = PlanRegister(plan: today) { _ in }
		let note = vault.appending(path: Self.storyNote)
		let before = try String(contentsOf: note, encoding: .utf8)
		let order = today.items.map(\.taskId)
		let first = try XCTUnwrap(today.unfinishedItems.first?.taskId)

		register.cut(first)
		XCTAssertFalse(today.items.map(\.taskId).contains(first), register.message ?? "")
		XCTAssertNil(today.lineState(of: first)?.scheduled)

		register.paste(after: true, anchor: today.unfinishedItems.last?.taskId)
		XCTAssertEqual(today.lineState(of: first)?.scheduled, "2026-10-23")
		XCTAssertEqual(today.unfinishedItems.last?.taskId, first)

		register.undo()
		register.undo()
		XCTAssertEqual(today.items.map(\.taskId), order)
		XCTAssertEqual(try String(contentsOf: note, encoding: .utf8), before, "undo restores the note byte for byte")
		XCTAssertNil(today.lineState(of: "task-999"))
		XCTAssertNotNil(today.apply(.reopen, to: "task-999"), "the core refuses an unknown task")
	}

	// MARK: Over free time

	func testAPlanBeyondTheFreeTimeShowsTheToastUntilDismissedAndRePlanAnswersIt() throws {
		let harness = try OnboardingHarness(self)
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspace = openWorkspace(harness, vault: vault)
		let lateAfternoon = Calendar.current.date(byAdding: .minute, value: 3 * 60 + 30, to: Self.friday)!
		let plan = DayPlanModel(now: { lateAfternoon })
		plan.load(from: workspace, date: "2026-10-23")
		XCTAssertEqual(plan.totals.freeMinutes, 60, "16:30 to the end of the working day at 17:30")
		XCTAssertEqual(plan.toast, PlanToast(message: "Over free time by 1h", offersReplan: true))

		plan.dismissToast()
		plan.reload()
		XCTAssertNil(plan.toast, "a dismissed toast stays away for the day")

		plan.schedule(.replan)
		XCTAssertEqual(plan.toast, PlanToast(message: "Over free time by 1h", offersReplan: true))
		XCTAssertNil(plan.items.first?.slot, "the task does not fit the hour that is left")

		plan.overrunWarning = .never
		plan.dismissToast()
		plan.schedule(.autoSchedule)
		XCTAssertEqual(plan.toast?.offersReplan, false)
		XCTAssertEqual(plan.items.first?.slot?.start.text, "09:00")
	}

	// MARK: Quick add

	func testTheConfirmationShowsWhatTheGrammarUnderstood() throws {
		let model = QuickAddModel(now: { Self.friday })
		try model.attach(workspacePath: TestFixtures.mockVault.path)
		model.text = "Review the coupon copy tomorrow 30m WEB-142"
		model.submit()
		let entry = try XCTUnwrap(model.entry)
		XCTAssertEqual(entry.kindTitle, "Task")
		XCTAssertEqual(entry.switchTitle, "switch to Event")
		XCTAssertEqual(entry.text, "Review the coupon copy")
		XCTAssertEqual(entry.storyTitle, "Checkout redesign")
		XCTAssertEqual(entry.chips, ["WEB-142", "Sat 24 Oct", "30m", "for you · Next Steps"])
		XCTAssertTrue(entry.id.hasPrefix("task-"))
		XCTAssertTrue(entry.canSave)

		model.text = "Final copy for the receipt WEB-157 waiting on Dana"
		model.submit()
		let waiting = try XCTUnwrap(model.entry)
		XCTAssertEqual(waiting.chips, ["WEB-157", "Fri 23 Oct", "for Dana · Pending Input"])

		model.switchKind()
		XCTAssertEqual(model.entry?.kindTitle, "Event")
		XCTAssertEqual(model.entry?.saveBlocker, "Events reach the calendar in a later version")
		model.switchKind()

		model.text = "Ask Tom about the analytics setup"
		model.submit()
		XCTAssertEqual(model.entry?.saveBlocker, "Pick a story with Adjust")
		model.adjust()
		XCTAssertTrue(model.isAdjusting)
		let story = try XCTUnwrap(model.stories.first { $0.number == "ACME-9" })
		model.setStory(story)
		XCTAssertTrue(model.setEstimate("1h 30m"))
		XCTAssertFalse(model.setEstimate("soon"))
		XCTAssertEqual(model.entry?.chips, ["ACME-9", "Fri 23 Oct", "1h 30m", "for you · Next Steps"])
		XCTAssertTrue(model.entry?.canSave == true)

		model.cancel()
		XCTAssertNil(model.entry)
		XCTAssertEqual(model.text, "Ask Tom about the analytics setup", "Esc keeps what was typed")
	}

	func testAnUnknownTicketCannotBeSavedAndSaveWritesOneLine() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let model = QuickAddModel(now: { Self.friday })
		try model.attach(workspacePath: vault.path)
		model.text = "Look at WEB-999"
		model.submit()
		XCTAssertEqual(model.entry?.saveBlocker, "WEB-999 is not a story here; pick one with Adjust")
		XCTAssertEqual(model.entry?.warnings, ["WEB-999 matches no story here"])
		XCTAssertFalse(model.save())

		let note = vault.appending(path: Self.storyNote)
		let before = try String(contentsOf: note, encoding: .utf8)
		model.text = "Note the decline codes 20m WEB-142"
		model.submit()
		let entry = try XCTUnwrap(model.entry)
		XCTAssertTrue(model.save())
		XCTAssertEqual(model.text, "")
		XCTAssertNil(model.entry)
		let after = try String(contentsOf: note, encoding: .utf8)
		let added = Set(after.components(separatedBy: "\n")).subtracting(before.components(separatedBy: "\n"))
		XCTAssertEqual(added, ["- [ ] Note the decline codes [id:: \(entry.id)] [created:: 2026-10-23] [scheduled:: 2026-10-23] [estimate:: 20m]"])
	}

	func testAnEstimateAdjustCannotReadIsNotSavedAsTheLastOneItCould() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let model = QuickAddModel(now: { Self.friday })
		try model.attach(workspacePath: vault.path)
		model.text = "Note the decline codes 20m WEB-142"
		model.submit()
		model.adjust()
		XCTAssertTrue(model.setEstimate("1h3"))
		XCTAssertFalse(model.setEstimate("1h3x"))
		XCTAssertFalse(model.save(), "the field no longer says 1h 3m")
		XCTAssertNotNil(model.entry)
		XCTAssertTrue(model.setEstimate("45m"))
		XCTAssertTrue(model.save())
		let note = try String(contentsOf: vault.appending(path: Self.storyNote), encoding: .utf8)
		XCTAssertTrue(note.contains("Note the decline codes [id:: "))
		XCTAssertTrue(note.contains("[scheduled:: 2026-10-23] [estimate:: 45m]"))
	}

	func testEstimatesAsTypedInAdjust() {
		XCTAssertEqual(QuickAddModel.minutes(in: "45m"), 45)
		XCTAssertEqual(QuickAddModel.minutes(in: "2h"), 120)
		XCTAssertEqual(QuickAddModel.minutes(in: "1h30m"), 90)
		XCTAssertEqual(QuickAddModel.minutes(in: "1.5h"), 90)
		XCTAssertNil(QuickAddModel.minutes(in: "1h75m"))
		XCTAssertNil(QuickAddModel.minutes(in: "0m"))
		XCTAssertNil(QuickAddModel.minutes(in: "later"))
		XCTAssertNil(QuickAddModel.minutes(in: "infh"), "not a number of hours")
		XCTAssertNil(QuickAddModel.minutes(in: "nanh"))
		XCTAssertNil(QuickAddModel.minutes(in: "1e18h"), "past any day")
		XCTAssertNil(QuickAddModel.minutes(in: "5000000000m"))
		XCTAssertEqual(QuickAddModel.minutes(in: "24h"), 24 * 60)
		XCTAssertEqual(QuickAddModel.minutes(in: "45min"), 45, "as the quick-add line reads it")
		XCTAssertNil(QuickAddModel.minutes(in: "1.5h30"))
		XCTAssertNil(QuickAddModel.minutes(in: "1e1h"))
	}

	func testALineRoutedToAnotherWorkspaceIsNotSavedHere() throws {
		let model = QuickAddModel(now: { Self.friday })
		let elsewhere = WorkspaceEntry(id: "ws-life", name: "Home life", sphere: .life, accent: nil, rootPath: "/nowhere", folders: [])
		try model.attach(workspacePath: TestFixtures.mockVault.path, workspaces: [elsewhere])
		model.text = "Call the bank /ws-life 10m WEB-142"
		model.submit()
		XCTAssertEqual(model.entry?.saveBlocker, "Routed to Home life; add it from that workspace")
		XCTAssertFalse(model.save())
	}

	func testAPastDayIsNotScheduled() throws {
		let harness = try OnboardingHarness(self)
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspace = openWorkspace(harness, vault: vault)
		let plan = DayPlanModel(now: { Self.friday })
		plan.load(from: workspace, date: "2026-10-22")
		XCTAssertTrue(plan.isPast)
		XCTAssertEqual(plan.items.map(\.taskId), ["task-2"])
		plan.schedule(.autoSchedule)
		XCTAssertEqual(plan.errorMessage, "Past days are not planned")
		XCTAssertNil(plan.items.first?.slot)
	}

	// MARK: All workspaces

	func testTheAllViewIsTheUnionOfEveryWorkspaceInSidebarOrder() throws {
		let harness = try OnboardingHarness(self)
		let work = try TestFixtures.copyOfMockVault(self)
		let life = try TestFixtures.copyOfMockVault(self)
		let settings = life.appending(path: ".vaultday/settings.json")
		var json = try TestFixtures.settingsJSON(in: life)
		json["name"] = "Home life"
		json["kind"] = "life"
		try JSONSerialization.data(withJSONObject: json).write(to: settings)

		let workspaces = WorkspacesModel(session: harness.session)
		XCTAssertNotNil(workspaces.addWorkspace(folder: life, sphere: .life), workspaces.message ?? "")
		XCTAssertNotNil(workspaces.addWorkspace(folder: work, sphere: .work), workspaces.message ?? "")
		let workspace = WorkspaceModel(session: harness.session)
		workspace.restoreAtLaunch()
		workspace.showAll()
		XCTAssertEqual(workspace.scope, .all)

		let all = AllWorkspacesModel(session: harness.session, now: { Self.friday })
		all.load(entries: workspaces.entries, openWorkspace: workspace, date: "2026-10-23")
		XCTAssertEqual(all.groups.map(\.name), ["Mock Studio", "Home life"], "work first, as the sidebar lists them")
		XCTAssertEqual(all.groups.map(\.sphere), [.work, .life])
		XCTAssertEqual(all.groups.map { $0.items.map(\.taskId) }, [["task-2"], ["task-2"]])
		let ids = all.groups.flatMap { $0.items.map(\.id) }
		XCTAssertEqual(Set(ids).count, ids.count, "the same task id in two workspaces stays two rows")
		XCTAssertEqual(all.groups.map(\.plannedMinutes), [120, 120])
		XCTAssertTrue(all.groups.allSatisfy { $0.problem == nil })
		XCTAssertEqual(all.title, "Friday 23 October")

		all.showDay(offset: 1)
		XCTAssertEqual(all.date, "2026-10-24")
		XCTAssertEqual(all.groups.map(\.items.count), [0, 0])
	}

	// MARK: Rituals

	func testEachRitualShowsWhenItIsNextDueAndTheSoonestIsMarked() {
		let times: [Ritual: DayTime] = [.planning: DayTime(text: "08:45")!, .shutdown: DayTime(text: "17:30")!, .weeklyReview: DayTime(text: "16:00")!]
		let rows = RitualSchedule.rows(
			RitualSchedule.rituals(for: .work),
			time: { times[$0]! },
			day: { $0 == .weeklyReview ? .friday : nil },
			now: Self.friday,
		)
		XCTAssertEqual(rows.map(\.title), ["Morning planning", "Shutdown", "Weekly review"])
		XCTAssertEqual(rows.map(\.whenText), ["08:45", "17:30", "Fri 16:00"])
		XCTAssertEqual(rows.filter(\.isNext).map(\.ritual), [.weeklyReview])
		let tomorrow = Calendar.current.date(byAdding: .day, value: 1, to: Self.friday)!
		XCTAssertTrue(Calendar.current.isDate(rows[0].next, inSameDayAs: tomorrow), "planning already passed today")
		XCTAssertEqual(RitualSchedule.rituals(for: .life), [.lightPlanning, .projectReview])
	}

	// MARK: Helpers

	private func item(_ id: String, state: PlanItem.State) -> PlanItem {
		PlanItem(id: id, taskId: id, text: id, storyNumber: nil, storyTitle: nil, isPendingInput: false, state: state, estimateMinutes: nil, assignedTo: nil, waitingDays: nil, subtasks: [], slot: nil)
	}
}
