import Foundation
import XCTest
@testable import VaultdayCore

/// The Ready step and the hand-off into the main window, and what a relaunch opens.
@MainActor
final class FinishAndRelaunchTests: XCTestCase {
	private func setUp(_ harness: OnboardingHarness, names: [String]) throws -> (OnboardingModel, WorkspacesModel, [WorkspaceEntry]) {
		let workspaces = WorkspacesModel(session: harness.session)
		var entries: [WorkspaceEntry] = []
		for (index, name) in names.enumerated() {
			let folder = try TestFixtures.emptyFolder(self, named: name)
			entries.append(try XCTUnwrap(workspaces.addWorkspace(folder: folder, sphere: index == names.count - 1 ? .life : .work, name: name)))
		}
		return (OnboardingModel(session: harness.session, workspaces: workspaces), workspaces, workspaces.entries)
	}

	private func walkToTheEnd(_ onboarding: OnboardingModel) {
		for _ in 0 ..< SetupStep.allCases.count { onboarding.goForward() }
	}

	func testEveryStepIsReachableAndOnlyTheLastOneFinishes() throws {
		let (onboarding, _, entries) = try setUp(try OnboardingHarness(self), names: ["Freelancing"])
		XCTAssertNil(onboarding.finish(workspaces: entries), "finishing belongs to the Ready step")
		XCTAssertFalse(onboarding.isFinished)
		var visited: [SetupStep] = [onboarding.current]
		while onboarding.canContinue {
			onboarding.goForward()
			visited.append(onboarding.current)
		}
		XCTAssertEqual(visited, SetupStep.allCases)
		XCTAssertTrue(onboarding.isLastStep)
		XCTAssertEqual(onboarding.continueTitle, "Open Vaultday")
		XCTAssertEqual(onboarding.progressText, "Step 12 of 12")
		XCTAssertTrue(onboarding.canGoBack, "Back stays available on the last step")
		onboarding.goBack()
		XCTAssertEqual(onboarding.current, .aiAssistance)
		XCTAssertFalse(onboarding.isLastStep)
	}

	func testFinishingMarksTheSetupDoneAndPicksTheFirstWorkspaceWhenNoneWasShown() throws {
		let harness = try OnboardingHarness(self)
		let (onboarding, _, entries) = try setUp(harness, names: ["Alpha", "Beta"])
		walkToTheEnd(onboarding)
		XCTAssertNil(harness.session.appState.lastSingleWorkspace())
		let opened = onboarding.finish(workspaces: entries)
		XCTAssertEqual(opened, entries.first?.id)
		XCTAssertTrue(onboarding.isFinished)
		XCTAssertEqual(harness.session.appState.lastSingleWorkspace(), entries.first?.id)

		let relaunched = try harness.relaunched()
		let reopened = OnboardingModel(session: relaunched.session, workspaces: WorkspacesModel(session: relaunched.session))
		XCTAssertTrue(reopened.isFinished)
	}

	func testFinishingKeepsTheLastSingleWorkspaceThatStillExists() throws {
		let harness = try OnboardingHarness(self)
		let (onboarding, _, entries) = try setUp(harness, names: ["Alpha", "Beta"])
		let beta = try XCTUnwrap(entries.last)
		try harness.session.edit { try $0.setLastSingleWorkspace(workspace: beta.id) }
		walkToTheEnd(onboarding)
		XCTAssertEqual(onboarding.finish(workspaces: entries), beta.id)
		try harness.session.edit { try $0.setLastSingleWorkspace(workspace: "ws-gone") }
		XCTAssertEqual(onboarding.finish(workspaces: entries), entries.first?.id, "a workspace that no longer exists is not opened")
	}

	func testFinishingWithNoWorkspaceStillFinishes() throws {
		let (onboarding, _, entries) = try setUp(try OnboardingHarness(self), names: [])
		walkToTheEnd(onboarding)
		XCTAssertNil(onboarding.finish(workspaces: entries))
		XCTAssertTrue(onboarding.isFinished)
	}

	func testTheSummaryReadsBackTheChoicesSphereBySphere() throws {
		let harness = try OnboardingHarness(self)
		let (_, _, entries) = try setUp(harness, names: ["Freelancing", "Personal"])
		let rhythm = RhythmModel(session: harness.session)
		rhythm.setTime("08:30", of: .planning)
		let accounts = AccountsStepModel(session: harness.session)
		accounts.refresh(workspaces: entries)
		accounts.set(.mail, isOn: true, workspace: entries[0].id)
		accounts.set(.calendar, isOn: true, workspace: entries[0].id)
		let ready = ReadyStepModel(session: harness.session, rhythm: rhythm)
		ready.refresh(workspaces: entries)

		let work = try XCTUnwrap(ready.lines[.work])
		XCTAssertEqual(work.first?.title, "Workspace · Freelancing")
		XCTAssertEqual(work.first?.detail, "1 folder")
		XCTAssertTrue(work.contains { $0.title == "Mail, Calendar" })
		XCTAssertTrue(work.contains { $0.title == "Weekdays 09:00 – 17:30" && $0.detail == "planning 08:30 · shutdown 17:30" })
		let life = try XCTUnwrap(ready.lines[.life])
		XCTAssertEqual(life.first?.title, "Workspace · Personal")
		XCTAssertTrue(life.contains { $0.title == "Evenings and weekends" && $0.detail.contains("project review Sun 18:00") })
		XCTAssertEqual(ready.syncLine, "Your notes stay on this Mac, mirrored to iCloud Drive for your other devices")
	}

	// MARK: Relaunch

	func testARelaunchOpensTheLastSingleWorkspace() throws {
		let harness = try OnboardingHarness(self)
		let (_, _, entries) = try setUp(harness, names: ["Alpha", "Beta"])
		let model = WorkspaceModel(session: harness.session)
		model.restoreAtLaunch()
		XCTAssertEqual(model.phase, .open)
		let beta = try XCTUnwrap(entries.last)
		try harness.session.edit { try $0.setLastSingleWorkspace(workspace: beta.id) }

		let relaunched = try harness.relaunched()
		let again = WorkspaceModel(session: relaunched.session)
		again.restoreAtLaunch()
		XCTAssertEqual(again.workspaceId, beta.id)
		XCTAssertEqual(again.name, "Beta")
		XCTAssertEqual(again.scope, .single)
		XCTAssertEqual(again.lastSingleWorkspaceId, beta.id)
	}

	func testShowingAllNeverReplacesTheLastSingleWorkspaceAndARelaunchNeverOpensOnAll() throws {
		let harness = try OnboardingHarness(self)
		let (_, _, entries) = try setUp(harness, names: ["Alpha", "Beta"])
		let beta = try XCTUnwrap(entries.last)
		try harness.session.edit { try $0.setLastSingleWorkspace(workspace: beta.id) }
		let model = WorkspaceModel(session: harness.session)
		model.restoreAtLaunch()
		XCTAssertEqual(model.workspaceId, beta.id)

		model.showAll()
		XCTAssertEqual(model.scope, .all)
		XCTAssertEqual(model.lastSingleWorkspaceId, beta.id, "All is not remembered")

		let relaunched = try harness.relaunched()
		let again = WorkspaceModel(session: relaunched.session)
		XCTAssertEqual(again.scope, .single)
		again.restoreAtLaunch()
		XCTAssertEqual(again.scope, .single)
		XCTAssertEqual(again.workspaceId, beta.id)
		again.showAll()
		again.restoreAtLaunch()
		XCTAssertEqual(again.scope, .single, "restoring always lands on one workspace")
	}

	func testAWorkspaceThatIsGoneFallsBackToTheFirstOneThatResolves() throws {
		let harness = try OnboardingHarness(self)
		let (_, _, entries) = try setUp(harness, names: ["Alpha", "Beta"])
		try harness.session.edit { try $0.setLastSingleWorkspace(workspace: "ws-removed") }
		let model = WorkspaceModel(session: harness.session)
		model.restoreAtLaunch()
		XCTAssertEqual(model.phase, .open)
		XCTAssertTrue(entries.map(\.id).contains(model.workspaceId))
		XCTAssertEqual(model.lastSingleWorkspaceId, model.workspaceId, "what opened is what is remembered")
	}
}
