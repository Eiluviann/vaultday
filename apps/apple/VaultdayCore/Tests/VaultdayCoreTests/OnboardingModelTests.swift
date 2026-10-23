import Foundation
import XCTest
@testable import VaultdayCore

/// The setup window's state: Back, Continue, jumping, persistence and Restore.
@MainActor
final class OnboardingModelTests: XCTestCase {
	private func model(_ harness: OnboardingHarness) -> (OnboardingModel, WorkspacesModel) {
		let workspaces = WorkspacesModel(session: harness.session)
		return (OnboardingModel(session: harness.session, workspaces: workspaces), workspaces)
	}

	func testItStartsOnWelcomeWithRestoreInPlaceOfBack() throws {
		let (onboarding, _) = model(try OnboardingHarness(self))
		XCTAssertEqual(onboarding.current, .welcome)
		XCTAssertFalse(onboarding.canGoBack)
		XCTAssertTrue(onboarding.isRestoreAvailable)
		XCTAssertEqual(onboarding.continueTitle, "Get started")
		XCTAssertEqual(onboarding.progressText, "Step 1 of 12")
		XCTAssertEqual(SetupStep.allCases.count, 12)
		XCTAssertEqual(SetupStep.allCases.map(\.title).first, "Welcome")
		XCTAssertEqual(SetupStep.allCases.filter(\.isDrawnInThisRelease).count, 6)
	}

	func testContinueAndBackMoveOneStepAndKeepTheFurthestReached() throws {
		let (onboarding, _) = model(try OnboardingHarness(self))
		onboarding.goForward()
		onboarding.goForward()
		onboarding.goForward()
		XCTAssertEqual(onboarding.current, .syncAndBackup)
		XCTAssertEqual(onboarding.continueTitle, "Continue")
		XCTAssertFalse(onboarding.isRestoreAvailable)
		onboarding.goBack()
		XCTAssertEqual(onboarding.current, .folders)
		XCTAssertEqual(onboarding.furthest, .syncAndBackup)
		XCTAssertEqual(onboarding.mark(of: .welcome), .done)
		XCTAssertEqual(onboarding.mark(of: .folders), .current)
		XCTAssertEqual(onboarding.mark(of: .syncAndBackup), .done, "a step reached stays a link back")
		XCTAssertEqual(onboarding.mark(of: .accounts), .upcoming)
		XCTAssertEqual(onboarding.progressText, "Step 3 of 12")
	}

	func testBackIsRefusedOnTheFirstStepAndContinueOnTheLast() throws {
		let (onboarding, _) = model(try OnboardingHarness(self))
		onboarding.goBack()
		XCTAssertEqual(onboarding.current, .welcome)
		for _ in 0 ..< 20 { onboarding.goForward() }
		XCTAssertEqual(onboarding.current, .ready)
		XCTAssertFalse(onboarding.canContinue)
		XCTAssertFalse(onboarding.isFinished, "finishing belongs to the last step's own action")
	}

	func testAStepNotReachedYetCannotBeJumpedTo() throws {
		let (onboarding, _) = model(try OnboardingHarness(self))
		onboarding.goForward()
		XCTAssertFalse(onboarding.jump(to: .accounts))
		XCTAssertEqual(onboarding.current, .workspaces)
		onboarding.goForward()
		XCTAssertTrue(onboarding.jump(to: .welcome))
		XCTAssertTrue(onboarding.jump(to: .folders))
		XCTAssertEqual(onboarding.current, .folders)
	}

	func testTheStepAndTheFurthestSurviveARelaunch() throws {
		let harness = try OnboardingHarness(self)
		let (onboarding, _) = model(harness)
		onboarding.goForward()
		onboarding.goForward()
		onboarding.goForward()
		onboarding.goBack()

		let (relaunched, _) = model(try harness.relaunched())
		XCTAssertEqual(relaunched.current, .folders)
		XCTAssertEqual(relaunched.furthest, .syncAndBackup)
	}

	func testRestoreFromAFolderTakesItsWorkspacesAndShowsTheWorkspacesStep() throws {
		let vaults = try TestFixtures.copyOfMockVault(self).deletingLastPathComponent()
		let (onboarding, workspaces) = model(try OnboardingHarness(self))
		let outcome = onboarding.restore(from: vaults)
		XCTAssertEqual(outcome, .restored(count: 1))
		XCTAssertEqual(onboarding.current, .workspaces)
		XCTAssertEqual(onboarding.restoreMessage, "Restored 1 workspace")
		XCTAssertEqual(workspaces.entries.map(\.name), ["Mock Studio"])
	}

	func testRestoreFromAFolderWithNoWorkspaceSaysSoAndStaysPut() throws {
		let empty = try TestFixtures.emptyFolder(self, named: "backup")
		let (onboarding, workspaces) = model(try OnboardingHarness(self))
		XCTAssertEqual(onboarding.restore(from: empty), .nothingFound)
		XCTAssertEqual(onboarding.current, .welcome)
		XCTAssertEqual(onboarding.restoreMessage, "No workspace found in that folder")
		XCTAssertEqual(workspaces.entries, [])
	}

	func testRestoreRefusesWorkspacesThatWouldOverlap() throws {
		let outer = try TestFixtures.emptyFolder(self, named: "clients")
		let inner = outer.appending(path: "acme", directoryHint: .isDirectory)
		let (onboarding, workspaces) = model(try OnboardingHarness(self))
		try FileManager.default.createDirectory(at: inner, withIntermediateDirectories: true)
		for (folder, name) in [(outer, "Clients"), (inner, "Acme")] {
			_ = try createWorkspace(root: folder.path, name: name, kind: .work)
		}
		let outcome = onboarding.restore(from: outer)
		guard case let .failed(reason) = outcome else { return XCTFail("expected a refusal, got \(outcome)") }
		XCTAssertTrue(reason.contains("overlap"), reason)
		XCTAssertEqual(onboarding.current, .welcome)
		XCTAssertEqual(workspaces.entries, [])
	}
}
