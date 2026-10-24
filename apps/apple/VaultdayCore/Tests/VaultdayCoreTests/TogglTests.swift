import Foundation
import XCTest
@testable import VaultdayCore

/// The Toggl and billing feature set in the app: the Settings panel's
/// model keeps the token in the Keychain only, the Configure sheet's billing
/// model keeps rates out of `settings.json`, a stopped session reaches the
/// queue once, and both Hours slots read the week. Nothing here reaches the
/// live Toggl service: the core refuses it while `allow_live` is off.
@MainActor
final class TogglTests: XCTestCase {
	private static let token = "synthetic-toggl-token-0042"
	/// A fixed day for the reads that need one, never the wall clock.
	private static let thursday = "2026-10-22"

	private func openWorkspace(_ harness: OnboardingHarness) throws -> WorkspaceModel {
		let workspace = WorkspaceModel(session: harness.session)
		workspace.grantFolder(try TestFixtures.copyOfMockVault(self))
		XCTAssertEqual(workspace.phase, .open)
		XCTAssertFalse(workspace.workspaceId.isEmpty)
		return workspace
	}

	private func setHourly(_ workspace: WorkspaceModel, _ harness: OnboardingHarness) {
		let billing = BillingSettingsModel(root: workspace.rootPath, workspaceId: workspace.workspaceId, session: harness.session)
		billing.model = .hourly
		billing.roundingMinutes = 6
		XCTAssertTrue(billing.save(), billing.errorMessage ?? "")
	}

	/// Every file under `folder` whose bytes hold `needle`.
	private nonisolated static func files(in folder: URL, containing needle: String) -> [String] {
		let bytes = Data(needle.utf8)
		let walker = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: nil)
		var found: [String] = []
		while let url = walker?.nextObject() as? URL {
			if let data = try? Data(contentsOf: url), data.range(of: bytes) != nil {
				found.append(url.path)
			}
		}
		return found
	}

	// MARK: Settings panel

	func testRefreshedPanelsFollowTheListAndKeepWhatIsTyped() throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let panel = { (id: String) in TogglSettingsModel(session: harness.session, workspaceId: id, workspaceName: id) }
		let work = panel(workspace.workspaceId)
		work.tokenDraft = "typed, not stored"
		let life = panel("ws-life")

		let hidden = TogglSettingsModel.refreshed([work, life], with: [panel(workspace.workspaceId)])
		XCTAssertEqual(hidden.map(\.workspaceId), [workspace.workspaceId], "the mode turned on: the life panel leaves")
		XCTAssertTrue(hidden.first === work, "the open panel stays")
		XCTAssertEqual(hidden.first?.tokenDraft, "typed, not stored")

		let shown = TogglSettingsModel.refreshed(hidden, with: [panel(workspace.workspaceId), panel("ws-life")])
		XCTAssertEqual(shown.map(\.workspaceId), [workspace.workspaceId, "ws-life"], "the mode turned off: it comes back")
		XCTAssertTrue(shown.first === work)
	}

	func testTheTokenGoesToTheKeychainOnlyAndThePanelSaysTokenStored() throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let toggl = TogglSettingsModel(session: harness.session, workspaceId: workspace.workspaceId, workspaceName: workspace.name)
		XCTAssertFalse(toggl.isEnabled)
		XCTAssertEqual(toggl.tokenStatusText, "No token stored")

		toggl.setEnabled(true)
		XCTAssertTrue(toggl.isEnabled && toggl.isOnForApp)
		toggl.tokenDraft = "   "
		toggl.storeToken()
		XCTAssertNotNil(toggl.errorMessage)
		XCTAssertFalse(toggl.hasToken)

		toggl.tokenDraft = Self.token
		toggl.storeToken()
		XCTAssertNil(toggl.errorMessage)
		XCTAssertEqual(toggl.tokenDraft, "", "the draft is forgotten once stored")
		XCTAssertEqual(toggl.tokenStatusText, "Token stored")
		let stored = try XCTUnwrap(harness.session.environment.secrets.get(id: SecretId(kind: .togglApiToken, name: workspace.workspaceId)))
		XCTAssertEqual(stored.value, Data(Self.token.utf8))
		XCTAssertFalse(stored.isSynchronisable, "the token stays on this Mac")

		XCTAssertEqual(Self.files(in: harness.container, containing: Self.token), [], "never in the app document")
		XCTAssertEqual(Self.files(in: URL(filePath: workspace.rootPath), containing: Self.token), [], "never in the vault or settings.json")
		let reopened = TogglSettingsModel(session: try harness.relaunched().session, workspaceId: workspace.workspaceId, workspaceName: workspace.name)
		XCTAssertTrue(reopened.hasToken)
		XCTAssertEqual(reopened.tokenDraft, "")

		XCTAssertFalse(toggl.isLiveAllowed)
		XCTAssertFalse(toggl.canFetchFromToggl, "live access is off in this build")
		toggl.fetchFromToggl()
		XCTAssertTrue(toggl.togglWorkspaces.isEmpty)
		XCTAssertNil(toggl.errorMessage)

		toggl.removeToken()
		XCTAssertEqual(toggl.tokenStatusText, "No token stored")
		XCTAssertNil(try harness.session.environment.secrets.get(id: SecretId(kind: .togglApiToken, name: workspace.workspaceId)))
	}

	func testTheMappingIsTypedByHandAndPersists() throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let toggl = TogglSettingsModel(session: harness.session, workspaceId: workspace.workspaceId, workspaceName: workspace.name)
		toggl.togglWorkspaceId = "4242"
		XCTAssertFalse(toggl.saveMapping(), "the feature is off for the workspace")
		toggl.setEnabled(true)
		toggl.togglWorkspaceId = "4242"
		toggl.defaultProjectId = "502"
		toggl.addStoryProject()
		toggl.storyProjects[0] = TogglStoryProject(story: "WEB-142", projectId: "501")
		XCTAssertTrue(toggl.saveMapping(), toggl.errorMessage ?? "")
		let reopened = TogglSettingsModel(session: try harness.relaunched().session, workspaceId: workspace.workspaceId, workspaceName: workspace.name)
		XCTAssertEqual(reopened.togglWorkspaceId, "4242")
		XCTAssertEqual(reopened.defaultProjectId, "502")
		XCTAssertEqual(reopened.storyProjects, [TogglStoryProject(story: "WEB-142", projectId: "501")])
		reopened.defaultProjectId = "five"
		XCTAssertFalse(reopened.saveMapping())
		XCTAssertEqual(reopened.errorMessage, "Toggl ids are whole numbers")
	}

	// MARK: Configure sheet

	func testTheBillingModelIsWrittenToSettingsAndTheRateToTheAppDocumentOnly() throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let billing = BillingSettingsModel(root: workspace.rootPath, workspaceId: workspace.workspaceId, session: harness.session)
		XCTAssertEqual(billing.model, BillingModelChoice.none)
		billing.model = .hourly
		billing.roundingMinutes = 15
		billing.isBillableByDefault = false
		billing.rate = "95.50"
		XCTAssertTrue(billing.save(), billing.errorMessage ?? "")
		let json = try TestFixtures.settingsJSON(in: URL(filePath: workspace.rootPath))
		let block = try XCTUnwrap(json["billing"] as? [String: Any])
		XCTAssertEqual(block["model"] as? String, "hourly")
		XCTAssertEqual(block["rounding_minutes"] as? Int, 15)
		XCTAssertEqual(block["billable_default"] as? Bool, false)
		XCTAssertNil(block["rate"])
		XCTAssertEqual(Self.files(in: URL(filePath: workspace.rootPath), containing: "9550"), [])

		let reopened = BillingSettingsModel(root: workspace.rootPath, workspaceId: workspace.workspaceId, session: try harness.relaunched().session)
		XCTAssertEqual(reopened.model, .hourly)
		XCTAssertEqual(reopened.rate, "95.50")
		XCTAssertEqual(reopened.currency, "EUR")

		reopened.model = .fte
		reopened.flatHours = "7.5"
		XCTAssertTrue(reopened.save())
		let fte = try XCTUnwrap(try TestFixtures.settingsJSON(in: URL(filePath: workspace.rootPath))["billing"] as? [String: Any])
		XCTAssertEqual(fte["model"] as? String, "fte")
		XCTAssertEqual(fte["flat_hours"] as? Double, 7.5)
		XCTAssertNil(fte["rounding_minutes"], "only the chosen model's keys")
		reopened.flatHours = "lots"
		XCTAssertFalse(reopened.save())
		XCTAssertFalse(BillingModelChoice.allCases.contains { $0.note.hasSuffix(".") })
	}

	// MARK: Queue

	func testAStoppedSessionIsQueuedOnceAndADrainSaysWhyNothingWasSent() async throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		setHourly(workspace, harness)
		let toggl = TogglSettingsModel(session: harness.session, workspaceId: workspace.workspaceId, workspaceName: workspace.name)
		toggl.setEnabled(true)
		let sessions = SessionModel()
		sessions.load(from: workspace)
		XCTAssertTrue(sessions.start(taskId: "task-2", story: "WEB-142", minutesAgo: 30))
		XCTAssertTrue(sessions.stop())
		XCTAssertNil(sessions.timeSinkMessage)

		let queue = TogglQueueModel()
		queue.load(source: workspace.togglQueueSource)
		XCTAssertEqual(queue.entries.count, 1)
		let entry = try XCTUnwrap(queue.entries.first)
		XCTAssertEqual(entry.description, "Wire the payment form to the gateway sandbox")
		XCTAssertEqual(entry.story, "WEB-142")
		XCTAssertEqual(entry.durationSeconds % 360, 0, "rounded up to 6 minutes")
		XCTAssertEqual(entry.state, .queued)
		XCTAssertEqual(queue.statusText, "1 queued")

		await queue.drainNow()
		XCTAssertEqual(queue.lastDrain?.notConnected, "Toggl is not connected: no token is stored for this workspace")
		XCTAssertEqual(queue.statusText, "Toggl is not connected: no token is stored for this workspace")

		toggl.tokenDraft = Self.token
		toggl.storeToken()
		await queue.drainNow()
		XCTAssertEqual(queue.lastDrain?.notConnected, "Toggl is not connected: allow_live is off")
		XCTAssertEqual(queue.entries.first?.attempts, 0, "nothing was attempted")

		// Another surface handing in the same stop, and a stop that finds
		// nothing running, add nothing.
		try workspace.sendToTimeSinks(sessionId: entry.key)
		XCTAssertFalse(sessions.stop())
		queue.reload()
		XCTAssertEqual(queue.entries.map(\.key), [entry.key])
	}

	func testWithTheFeatureOffAStopQueuesNothing() throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		setHourly(workspace, harness)
		let sessions = SessionModel()
		sessions.load(from: workspace)
		XCTAssertTrue(sessions.start(taskId: "task-2", story: "WEB-142", minutesAgo: 30))
		XCTAssertTrue(sessions.stop())
		let queue = TogglQueueModel()
		queue.load(source: workspace.togglQueueSource)
		XCTAssertEqual(queue.entries, [])
		XCTAssertEqual(queue.statusText, "Nothing queued")
	}

	// MARK: Hours slots

	func testTheSidePanelSlotSaysNotConnectedWhileOffAndShowsTheQueuedDayOnceOn() throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		setHourly(workspace, harness)
		let hours = HoursModel()
		hours.load(from: workspace, date: Self.thursday)
		XCTAssertFalse(hours.togglWeek.isConnected)
		XCTAssertEqual(hours.togglWeek.statusText, "Toggl not connected")
		XCTAssertEqual(hours.togglWeek.visibleDays, [])

		TogglSettingsModel(session: harness.session, workspaceId: workspace.workspaceId, workspaceName: workspace.name).setEnabled(true)
		hours.load(from: workspace, date: Self.thursday)
		XCTAssertTrue(hours.togglWeek.isConnected)
		XCTAssertEqual(hours.togglWeek.statusText, "Nothing for Toggl this week yet")
		XCTAssertEqual(hours.togglWeek.days.count, 7)

		let sessions = SessionModel()
		sessions.load(from: workspace)
		XCTAssertTrue(sessions.start(taskId: "task-2", story: "WEB-142", minutesAgo: 1))
		XCTAssertTrue(sessions.stop())
		// The core stamps the session with its own clock and queues it on the
		// local day it started, so the week to read is that day's, not the
		// day the test began: a minute before midnight they differ.
		let startedAt = try XCTUnwrap(sessions.sessions.last?.startedAt)
		let sessionDay = DayText.string(startedAt)
		hours.load(from: workspace, date: sessionDay)
		let day = try XCTUnwrap(hours.togglWeek.visibleDays.first)
		XCTAssertEqual(day.date, sessionDay)
		XCTAssertEqual(day.state, .queued)
		XCTAssertEqual(day.stateText, "Queued")
		XCTAssertNil(hours.togglWeek.statusText)
	}

	func testTheStorySlotAsksForTheOpenStorysWeekAndReadsFlatDays() throws {
		let source = FakeTogglWeek(snapshot: TogglWeekSnapshot(isConnected: true, days: [
			TogglWeekDay(date: "2026-10-19", weekday: "Mon", seconds: 8 * 3600, isFlat: true, state: .pushed),
			TogglWeekDay(date: "2026-10-20", weekday: "Tue", seconds: 0, isFlat: false, state: .empty),
			TogglWeekDay(date: "2026-10-22", weekday: "Thu", seconds: 8 * 3600, isFlat: true, state: .queued),
		]))
		var parts = DateComponents()
		parts.year = 2026
		parts.month = 10
		parts.day = 22
		parts.hour = 12
		let thursday = try XCTUnwrap(Calendar.current.date(from: parts))
		let story = StoryModel(now: { thursday })
		story.attach(workspacePath: TestFixtures.mockVault.path)
		story.attachToggl(source: source)
		story.open(number: "WEB-142")
		XCTAssertEqual(source.asked.last?.weekStart, "2026-10-19", "the Monday of the open day's week")
		XCTAssertEqual(source.asked.last?.story, "WEB-142")
		XCTAssertEqual(story.togglWeek.visibleDays.map(\.title), ["Mon · 8h flat", "Thu · 8h flat"])
		XCTAssertEqual(story.togglWeek.visibleDays.map(\.stateText), ["Pushed", "Queued"])

		story.attachToggl(source: nil)
		XCTAssertEqual(story.togglWeek.statusText, "Toggl not connected")
		story.attachToggl(source: FakeTogglWeek(snapshot: TogglWeekSnapshot(isConnected: false, days: [])))
		XCTAssertEqual(story.togglWeek.statusText, "Toggl not connected")
		story.close()
		XCTAssertEqual(story.togglWeek.days, [])
	}

	func testTheWeekStartsOnMondayAndTheCopyEndsWithoutAPeriod() {
		var parts = DateComponents()
		parts.year = 2026
		parts.month = 10
		parts.day = 25
		parts.hour = 9
		let sunday = Calendar.current.date(from: parts)!
		XCTAssertEqual(TogglWeekModel.weekStart(of: sunday), "2026-10-19")
		for text in [TogglWeekModel.title, TogglWeekModel.notConnectedText, TogglWeekModel.emptyText, TogglSettingsModel.tokenStoredText, TogglSettingsModel.typedByHandNote] {
			XCTAssertFalse(text.hasSuffix("."), text)
		}
		XCTAssertEqual(TogglWeekDay.weekday(of: "2026-10-23"), "Fri")
	}
}

/// A Toggl week that records what it was asked for.
private final class FakeTogglWeek: TogglWeekSource {
	let snapshot: TogglWeekSnapshot
	private(set) var asked: [(weekStart: String, story: String?)] = []

	init(snapshot: TogglWeekSnapshot) {
		self.snapshot = snapshot
	}

	func togglWeek(startingOn weekStart: String, story: String?) throws -> TogglWeekSnapshot {
		asked.append((weekStart, story))
		return snapshot
	}
}
