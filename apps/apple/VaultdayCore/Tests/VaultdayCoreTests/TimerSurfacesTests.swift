import CoreGraphics
import Foundation
import XCTest
@testable import VaultdayCore

/// The timer surfaces over the mock vault: Space pauses and resumes one
/// session; a session started from the menu bar, paused in the mini tracker
/// and stopped through the localhost API (as `vaultday timer stop` does) is
/// one session with one day-log line; "Continue latest", "Start minutes
/// ago" and the long-timer check; the menu bar's list and the tracker's
/// placement.
@MainActor
final class TimerSurfacesTests: XCTestCase {
	private static let token = String(repeating: "5e", count: 32)
	private static let paymentForm = "Wire the payment form to the gateway sandbox"
	/// Friday 23 October 2026, 13:00: task-2 is on that day's list, overdue.
	private static let friday: Date = {
		var parts = DateComponents()
		parts.year = 2026
		parts.month = 10
		parts.day = 23
		parts.hour = 13
		return Calendar.current.date(from: parts) ?? Date()
	}()

	private func todayPlan(_ workspace: WorkspaceModel) -> DayPlanModel {
		let dayPlan = DayPlanModel(now: { Self.friday })
		dayPlan.load(from: workspace, date: "2026-10-23")
		return dayPlan
	}

	private func openWorkspace(_ harness: OnboardingHarness) throws -> (WorkspaceModel, URL) {
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspace = WorkspaceModel(session: harness.session)
		workspace.grantFolder(vault)
		XCTAssertEqual(workspace.phase, .open)
		return (workspace, vault)
	}

	private func sessionLines(in vault: URL, sessionId: String) throws -> [String] {
		let log = vault.appending(path: "Log/\(DayText.string(Date())).md")
		guard FileManager.default.fileExists(atPath: log.path) else { return [] }
		return try String(contentsOf: log, encoding: .utf8)
			.components(separatedBy: "\n")
			.filter { $0.contains("[id:: \(sessionId)]") }
	}

	func testSpacePausesAndResumesAsOneSession() throws {
		let harness = try OnboardingHarness(self)
		let (workspace, _) = try openWorkspace(harness)
		let keymap = try Keymap.parse(String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)).get()
		for scheme in KeyScheme.allCases {
			XCTAssertEqual(KeyDispatcher(keymap: keymap, scheme: scheme).handle(KeyStroke(key: "space")), .action(.toggleTimer), "\(scheme)")
		}
		let sessions = SessionModel()
		sessions.load(from: workspace)
		XCTAssertTrue(sessions.start(taskId: "task-5", story: "WEB-142"))
		let id = try XCTUnwrap(sessions.openSession?.id)

		XCTAssertTrue(sessions.togglePause())
		XCTAssertEqual(sessions.openSession?.isPaused, true)
		XCTAssertTrue(sessions.togglePause())
		XCTAssertEqual(sessions.openSession?.isPaused, false)
		XCTAssertTrue(sessions.togglePause())
		XCTAssertTrue(sessions.togglePause())

		XCTAssertEqual(sessions.sessions.map(\.id), [id], "pauses stay inside the one session")
		XCTAssertEqual(sessions.openSession?.id, id)
	}

	func testMenuBarStartMiniTrackerPauseAndCLIStopLogOnce() async throws {
		let harness = try OnboardingHarness(self)
		let (workspace, vault) = try openWorkspace(harness)
		let sessions = SessionModel()
		sessions.load(from: workspace)
		let dayPlan = todayPlan(workspace)

		// The menu-bar extra: ⏎ on the picked row starts it.
		let picker = TimerPickerModel()
		picker.load(recentTaskIds: sessions.recentTaskIds(limit: 5), items: dayPlan.items)
		let pick = try XCTUnwrap(picker.picks.first { $0.taskId == "task-2" })
		XCTAssertTrue(sessions.start(taskId: pick.taskId, story: pick.story))
		let id = try XCTUnwrap(sessions.openSession?.id)

		// The mini tracker's pause button, then Space to resume.
		XCTAssertTrue(sessions.togglePause())
		XCTAssertTrue(sessions.togglePause())

		// `vaultday timer stop`: POST /v1/timer/stop with the token.
		let backend = AppTimerAPIBackend(sessions: sessions, dayPlan: dayPlan, workspaceName: { workspace.name }, workloadMinutes: { 480 })
		var changes = 0
		backend.onChange = { changes += 1 }
		let server = LocalAPIServer(router: TimerAPIRouter(token: Self.token, backend: backend))
		let port = try await server.start(port: 0)
		defer { server.stop() }

		let status = try await curl(port: port, path: "/v1/status", token: Self.token)
		let running = try JSONDecoder().decode(TimerAPIStatus.self, from: status.body)
		XCTAssertEqual(running.timer?.sessionId, id)
		XCTAssertEqual(running.timer?.text, Self.paymentForm)

		let stopped = try await curl(port: port, path: "/v1/timer/stop", method: "POST", token: Self.token)
		XCTAssertEqual(stopped.status, 200)
		let answer = try JSONDecoder().decode(TimerAPIStatus.self, from: stopped.body)
		XCTAssertNil(answer.timer)
		let logLine = try XCTUnwrap(answer.logLine)
		XCTAssertTrue(logLine.contains("session · \(Self.paymentForm) · 0m [story:: WEB-142] [kind:: session] [id:: \(id)] [task:: task-2]"), logLine)

		XCTAssertEqual(sessions.sessions.map(\.id), [id], "one session")
		XCTAssertNil(sessions.openSession, "every surface reads it stopped")
		XCTAssertEqual(try sessionLines(in: vault, sessionId: id), [logLine], "logged once")
		XCTAssertEqual(changes, 1)

		let again = try await curl(port: port, path: "/v1/timer/stop", method: "POST", token: Self.token)
		XCTAssertEqual(again.status, 409)
		XCTAssertEqual(try sessionLines(in: vault, sessionId: id).count, 1, "a second stop logs nothing")
	}

	func testTheAPIStartsByTextAndContinuesTheLatest() async throws {
		let harness = try OnboardingHarness(self)
		let (workspace, _) = try openWorkspace(harness)
		let sessions = SessionModel()
		sessions.load(from: workspace)
		let dayPlan = todayPlan(workspace)
		let backend = AppTimerAPIBackend(sessions: sessions, dayPlan: dayPlan, workspaceName: { workspace.name }, workloadMinutes: { 480 })

		guard case .failure(let nothing) = backend.continueTimer() else { return XCTFail("nothing to continue") }
		XCTAssertEqual(nothing.kind, .invalid)
		guard case .failure(let unknown) = backend.startTimer(target: "lunch with nobody", minutesAgo: nil) else { return XCTFail("no match") }
		XCTAssertEqual(unknown.kind, .notFound)

		let started = try backend.startTimer(target: "payment gateway", minutesAgo: 5).get()
		XCTAssertEqual(started.timer?.taskId, "task-2")
		XCTAssertGreaterThanOrEqual(started.timer?.elapsedSeconds ?? 0, 300, "started five minutes ago")
		guard case .failure(let twice) = backend.startTimer(target: "task-2", minutesAgo: nil) else { return XCTFail("already runs") }
		XCTAssertEqual(twice.kind, .conflict)

		let stopped = try backend.stopTimer().get()
		XCTAssertTrue(stopped.logLine?.contains(" · 5m ") == true, stopped.logLine ?? "")
		let continued = try backend.continueTimer().get()
		XCTAssertEqual(continued.timer?.taskId, "task-2")
		XCTAssertNotEqual(continued.timer?.sessionId, started.timer?.sessionId)
		XCTAssertEqual(sessions.latestTimerSession?.id, continued.timer?.sessionId)
		XCTAssertEqual(sessions.recentTaskIds(limit: 3), ["task-2"])
	}

	func testTheLongTimerAsksOncePerSession() throws {
		let harness = try OnboardingHarness(self)
		let (workspace, _) = try openWorkspace(harness)
		let sessions = SessionModel()
		sessions.load(from: workspace)
		let alert = RecordingAlert()
		XCTAssertFalse(sessions.checkLongTimer(limitMinutes: 15, alert: alert), "nothing runs")
		XCTAssertTrue(sessions.start(taskId: "task-2", story: nil, minutesAgo: 20))
		XCTAssertFalse(sessions.checkLongTimer(limitMinutes: 30, alert: alert))
		XCTAssertTrue(sessions.checkLongTimer(limitMinutes: 15, alert: alert))
		XCTAssertFalse(sessions.checkLongTimer(limitMinutes: 15, alert: alert), "asked once")
		XCTAssertEqual(alert.asked.map(\.0), [sessions.openSession?.id].compactMap(\.self))
		XCTAssertGreaterThanOrEqual(alert.asked.first?.1 ?? 0, 20 * 60)
	}

	func testTheLongTimerLimitIsASettingAndTheCheckUsesTheEarlierLimit() throws {
		let harness = try OnboardingHarness(self)
		let rhythm = RhythmModel(session: harness.session)
		XCTAssertEqual(rhythm.longTimerLimitMinutes, 180)
		XCTAssertEqual(rhythm.longTimerCheckMinutes, 180)
		XCTAssertFalse(rhythm.setLongTimerLimit(minutes: 5))
		XCTAssertNotNil(rhythm.issues["longTimer"])
		XCTAssertTrue(rhythm.setLongTimerLimit(minutes: 600))
		XCTAssertEqual(rhythm.longTimerCheckMinutes, 480, "the workload threshold comes first")
		XCTAssertEqual(RhythmModel(session: harness.session).longTimerLimitMinutes, 600, "stored in the app document")
	}

	func testTheMenuBarListPutsRecentTasksFirstAndMovesWithTheArrows() {
		let items = [
			planItem("task-1", "Draft the brief"),
			planItem("task-2", "Wire the payment form"),
			planItem("task-3", "Done already", isDone: true),
			planItem("task-4", "Write the checkout test"),
		]
		let picker = TimerPickerModel()
		picker.load(recentTaskIds: ["task-4", "task-3", "task-9"], items: items)
		XCTAssertEqual(picker.picks.map(\.taskId), ["task-4", "task-1", "task-2"])
		XCTAssertEqual(picker.picks.map(\.isRecent), [true, false, false])
		XCTAssertEqual(picker.selectedPick?.taskId, "task-4")
		picker.moveSelection(by: 1)
		picker.moveSelection(by: 1)
		picker.moveSelection(by: 1)
		XCTAssertEqual(picker.selectedPick?.taskId, "task-2", "stops at the end")
		picker.moveSelection(by: -5)
		XCTAssertEqual(picker.selectedPick?.taskId, "task-4")
		picker.beginNewEntry()
		XCTAssertTrue(picker.isComposing)
		picker.draft = "checkout"
		XCTAssertEqual(TimerTargetResolver.resolve(picker.draft, items: items), TimerTarget(taskId: "task-4", story: "WEB-142"))
		picker.endNewEntry()
		XCTAssertFalse(picker.isComposing)
	}

	func testTypedTargetsResolveByIdStoryOrText() {
		let items = [planItem("task-1", "Draft the brief"), planItem("task-4", "Write the checkout test")]
		XCTAssertEqual(TimerTargetResolver.resolve("task-1", items: items), TimerTarget(taskId: "task-1", story: "WEB-142"))
		XCTAssertEqual(TimerTargetResolver.resolve("task-77", items: items), TimerTarget(taskId: "task-77", story: nil))
		XCTAssertEqual(TimerTargetResolver.resolve("web-142", items: items), TimerTarget(taskId: nil, story: "WEB-142"))
		XCTAssertEqual(TimerTargetResolver.resolve("checkout write", items: items)?.taskId, "task-4")
		XCTAssertNil(TimerTargetResolver.resolve("  ", items: items))
		XCTAssertNil(TimerTargetResolver.resolve("lunch", items: items))
	}

	func testTheMiniTrackerSnapsToTheNearestEdgeAndHidesBehindTheMainWindow() {
		let screen = CGRect(x: 0, y: 0, width: 1440, height: 900)
		let size = MiniTrackerLayout.collapsedSize
		XCTAssertEqual(size, CGSize(width: 400, height: 56))
		let nearLeft = MiniTrackerLayout.snappedOrigin(for: CGRect(origin: CGPoint(x: 30, y: 400), size: size), in: screen)
		XCTAssertEqual(nearLeft, CGPoint(x: 12, y: 400))
		let nearTop = MiniTrackerLayout.snappedOrigin(for: CGRect(origin: CGPoint(x: 600, y: 820), size: size), in: screen)
		XCTAssertEqual(nearTop, CGPoint(x: 600, y: 900 - 12 - 56))
		let offscreen = MiniTrackerLayout.snappedOrigin(for: CGRect(origin: CGPoint(x: 1300, y: -40), size: size), in: screen)
		XCTAssertEqual(offscreen, CGPoint(x: 1440 - 12 - 400, y: 12), "pulled back inside, onto the nearest edge")

		XCTAssertTrue(MiniTrackerLayout.isVisible(hasOpenSession: true, isEnabled: true, isMainWindowFrontmost: false))
		XCTAssertFalse(MiniTrackerLayout.isVisible(hasOpenSession: true, isEnabled: true, isMainWindowFrontmost: true))
		XCTAssertFalse(MiniTrackerLayout.isVisible(hasOpenSession: false, isEnabled: true, isMainWindowFrontmost: false))
		XCTAssertFalse(MiniTrackerLayout.isVisible(hasOpenSession: true, isEnabled: false, isMainWindowFrontmost: false))
	}

	private func planItem(_ taskId: String, _ text: String, isDone: Bool = false) -> PlanItem {
		PlanItem(
			id: taskId,
			taskId: taskId,
			text: text,
			storyNumber: "WEB-142",
			storyTitle: "Checkout redesign",
			isPendingInput: false,
			state: isDone ? .done : .open,
			estimateMinutes: nil,
			assignedTo: nil,
			waitingDays: nil,
			subtasks: [],
		)
	}
}

@MainActor
private final class RecordingAlert: LongTimerAlerting {
	private(set) var asked: [(String, Int)] = []

	func askStillWorking(sessionId: String, elapsedSeconds: Int) {
		asked.append((sessionId, elapsedSeconds))
	}
}
