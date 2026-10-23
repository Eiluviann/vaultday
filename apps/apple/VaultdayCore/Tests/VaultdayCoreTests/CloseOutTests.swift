import Foundation
import XCTest
@testable import VaultdayCore

/// The week close-out in the app: the steps settle and advance in order,
/// the Monday alert fires once, the table is byte-exact TSV and the ticks
/// survive a relaunch. Everything runs on a copy of the synthetic mock vault
/// with the synthetic `Leave.md` of the two-workspaces fixture; Toggl stays
/// off, so its step is the dry run and nothing is sent anywhere.
@MainActor
final class CloseOutTests: XCTestCase {
	private static let leaveRoot = TestFixtures.repoRoot.appending(path: "core/fixtures/two-workspaces").path
	private static let tsv = "Story\tTitle\t2026-10-19\t2026-10-20\t2026-10-22\t2026-10-23\tTotal\nWEB-142\tCheckout redesign\t8.00\t\t\t\t8.00\n"

	/// A calendar holding one attended meeting on WEB-142, Monday 19 October 10:00 to 12:00.
	private final class MeetingCalendar: CalendarPort, @unchecked Sendable {
		func events(from _: String, to _: String) throws -> [CalendarEvent] {
			[CalendarEvent(id: "ev-1", calendar: "Work", title: "WEB-142 refinement", start: "2026-10-19T10:00:00", end: "2026-10-19T12:00:00", meetingKey: "ev-1", isAttended: true)]
		}

		func writeTimeBox(calendar _: String, title _: String, timeBox _: CalendarTimeBox) throws {}

		func deleteTimeBox(calendar _: String, id _: String) throws {}
	}

	private func openFteWorkspace(_ harness: OnboardingHarness, vault: URL? = nil) throws -> (WorkspaceModel, URL) {
		let folder = try vault ?? TestFixtures.copyOfMockVault(self)
		let workspace = WorkspaceModel(session: harness.session)
		workspace.grantFolder(folder)
		XCTAssertEqual(workspace.phase, .open)
		if vault == nil {
			let billing = BillingSettingsModel(root: workspace.rootPath, workspaceId: workspace.workspaceId, session: harness.session)
			billing.model = .fte
			XCTAssertTrue(billing.save(), billing.errorMessage ?? "")
			let settings = CloseOutSettingsModel(root: workspace.rootPath)
			settings.hasDeadline = true
			settings.deadlineDay = .monday
			settings.deadlineTime = "08:00"
			XCTAssertTrue(settings.save(), settings.errorMessage ?? "")
			let state = try XCTUnwrap(workspace.state)
			let handle = try XCTUnwrap(workspace.vault)
			XCTAssertEqual(try state.recordEndedMeetings(workspace: handle, calendar: MeetingCalendar(), date: "2026-10-19"), 1)
			_ = try state.flush()
		}
		return (workspace, folder)
	}

	private func openCloseOut(_ workspace: WorkspaceModel, copied: @escaping @MainActor (String) -> Void = { _ in }) throws -> CloseOutModel {
		let model = CloseOutModel(copy: copied)
		model.open(source: try XCTUnwrap(workspace.closeOutSource(leaveRoot: Self.leaveRoot)), kind: .week, date: "2026-10-21")
		return model
	}

	func testTheStepsSettleInOrderAndTheWeekClosesWithTheStatusLine() throws {
		let harness = try OnboardingHarness(self)
		let (workspace, folder) = try openFteWorkspace(harness)
		var copied: [String] = []
		let model = try openCloseOut(workspace) { copied.append($0) }
		XCTAssertTrue(model.isPresented)
		XCTAssertEqual(model.title, "Week close-out · Mon 19 – Fri 23")
		XCTAssertEqual(model.steps.map(\.kind), [.toggl, .allocate, .table, .close])
		XCTAssertEqual(model.steps.map(\.status), [.skipped, .open, .open, .open])
		XCTAssertEqual(model.steps.map(\.title), ["Toggl flat entries", "Allocate 8 h per day", "Time sheet", "Close the week"])
		XCTAssertEqual(model.currentStep, .allocate, "Toggl is off: its step is a dry run and settled")
		let snapshot = try XCTUnwrap(model.snapshot)
		XCTAssertFalse(snapshot.isTogglOn)
		XCTAssertEqual(snapshot.flat.map { $0.stateText(isTogglOn: false) }, ["Dry run", "Dry run", "", "Dry run", "Dry run"])
		XCTAssertEqual(snapshot.flat[2].title, "Wed 21 · leave")
		XCTAssertEqual(snapshot.missing, ["Allocation not confirmed for 4 days", "1 table row not ticked", "The week is not closed"])

		// Closing early is refused and changes nothing.
		model.select(.close)
		model.close(nowLocal: "2026-10-23T16:00")
		XCTAssertNotNil(model.errorMessage)
		XCTAssertNil(model.closedLine)
		model.select(.allocate)

		// Only the story worked that day can be adjusted.
		model.setMinutes(60, story: "WEB-142", day: "2026-10-20")
		XCTAssertNotNil(model.errorMessage)
		XCTAssertTrue(model.draft.isEmpty)

		model.confirmAllocation()
		XCTAssertNil(model.errorMessage)
		XCTAssertEqual(model.currentStep, .table)
		model.copyTable()
		XCTAssertEqual(copied, [Self.tsv])
		XCTAssertTrue(model.isTableCopied)
		let row = try XCTUnwrap(model.snapshot?.table.rows.first)
		model.toggleTick(row)
		XCTAssertEqual(model.snapshot?.table.rows.first?.isTicked, true)
		XCTAssertEqual(model.currentStep, .close)
		XCTAssertEqual(model.snapshot?.canClose, true)

		model.close(nowLocal: "2026-10-23T16:05")
		XCTAssertNil(model.errorMessage)
		XCTAssertEqual(model.closedLine, "- 16:05 status · week closed [kind:: status]")
		XCTAssertTrue(model.isFinished)
		XCTAssertTrue(model.steps.allSatisfy(\.isSettled))
		let log = try String(contentsOf: folder.appending(path: "Log/2026-10-23.md"), encoding: .utf8)
		XCTAssertEqual(log.components(separatedBy: "status · week closed").count - 1, 1)
		model.close(nowLocal: "2026-10-23T16:10")
		XCTAssertNil(model.closedLine, "a closed week writes no second line")
	}

	func testTheTableIsByteExactTsvAndTheTicksSurviveARelaunch() throws {
		let harness = try OnboardingHarness(self)
		let (workspace, folder) = try openFteWorkspace(harness)
		let model = try openCloseOut(workspace)
		let table = try XCTUnwrap(model.snapshot?.table)
		XCTAssertEqual(table.text, Self.tsv)
		XCTAssertEqual(Array(table.text.utf8).filter { $0 == 0x09 }.count, 12)
		XCTAssertEqual(table.columns, ["Story", "Title", "2026-10-19", "2026-10-20", "2026-10-22", "2026-10-23", "Total"])
		model.toggleTick(try XCTUnwrap(table.rows.first))
		XCTAssertEqual(model.snapshot?.table.openRows, 0)

		let relaunched = try harness.relaunched()
		let (reopened, _) = try openFteWorkspace(relaunched, vault: folder)
		let again = try openCloseOut(reopened)
		XCTAssertEqual(again.snapshot?.table.rows.first?.isTicked, true, "the tick is in the workspace document")
		again.toggleTick(try XCTUnwrap(again.snapshot?.table.rows.first))
		XCTAssertEqual(again.snapshot?.table.rows.first?.isTicked, false)
	}

	func testTheMondayAlertFiresOnceAndTheHeaderOffersTheWeek() throws {
		let harness = try OnboardingHarness(self)
		let (workspace, _) = try openFteWorkspace(harness)
		let source = try XCTUnwrap(workspace.closeOutSource(leaveRoot: Self.leaveRoot))
		let due = CloseOutDueModel()
		due.refresh(source: source, nowLocal: "2026-10-23T13:59")
		XCTAssertEqual(due.due?.date, "2026-10-12", "before Friday 14:00 the week before is offered")
		due.refresh(source: source, nowLocal: "2026-10-23T14:00")
		XCTAssertEqual(due.due?.title, "Week close-out")
		XCTAssertEqual(due.due?.date, "2026-10-19")
		XCTAssertEqual(due.due?.timeText, "Due Mon 08:00")

		due.checkAlert(source: source, nowLocal: "2026-10-26T07:29")
		XCTAssertNil(due.alert)
		due.checkAlert(source: source, nowLocal: "2026-10-26T07:30")
		let alert = try XCTUnwrap(due.alert)
		XCTAssertEqual(alert.weekStart, "2026-10-19")
		XCTAssertEqual(alert.title, "Week of Mon 19 still open")
		XCTAssertEqual(alert.missing, ["Allocation not confirmed for 4 days", "1 table row not ticked", "The week is not closed"])
		due.dismissAlert()
		due.checkAlert(source: source, nowLocal: "2026-10-26T09:00")
		XCTAssertNil(due.alert, "once per week")
		let other = try XCTUnwrap(workspace.closeOutSource(leaveRoot: Self.leaveRoot))
		due.checkAlert(source: other, nowLocal: "2026-10-27T07:30")
		XCTAssertNil(due.alert, "recorded in the document, not in the model")
	}

	func testSettingsRoundTripWithoutSecretsAndTheLeaveSourceResolves() throws {
		let harness = try OnboardingHarness(self)
		let folder = try TestFixtures.copyOfMockVault(self)
		let workspace = WorkspaceModel(session: harness.session)
		workspace.grantFolder(folder)
		let settings = CloseOutSettingsModel(root: workspace.rootPath)
		XCTAssertFalse(settings.hasDeadline)
		settings.hasDeadline = true
		settings.deadlineDay = .friday
		settings.deadlineTime = "25:00"
		XCTAssertFalse(settings.save())
		XCTAssertNotNil(settings.errorMessage)
		settings.deadlineTime = "17:30"
		settings.leaveSource = "Home"
		XCTAssertTrue(settings.save())
		let reread = CloseOutSettingsModel(root: workspace.rootPath)
		XCTAssertEqual(reread.deadlineDay, .friday)
		XCTAssertEqual(reread.deadlineTime, "17:30")
		XCTAssertEqual(reread.leaveSource, "Home")
		let json = try String(contentsOf: folder.appending(path: ".vaultday/settings.json"), encoding: .utf8)
		XCTAssertTrue(json.contains(#""log_deadline""#))
		XCTAssertTrue(json.contains(#""x_future_setting""#), "unknown keys are kept")

		let entries = [
			WorkspaceEntry(id: "ws-1", name: "Mock Studio", sphere: .work, accent: nil, rootPath: workspace.rootPath, folders: []),
			WorkspaceEntry(id: "ws-2", name: "Home", sphere: .life, accent: nil, rootPath: Self.leaveRoot, folders: []),
		]
		XCTAssertEqual(LeaveSource.root(for: workspace.rootPath, among: entries), Self.leaveRoot)
		let days = try LeaveSource.workingWeek(of: "2026-10-21", leaveRoot: Self.leaveRoot)
		XCTAssertEqual(days.map(\.kind), [.working, .working, .leave, .working, .working])
	}
}
