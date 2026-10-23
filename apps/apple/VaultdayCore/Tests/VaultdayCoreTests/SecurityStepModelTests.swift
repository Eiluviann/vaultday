import Foundation
import XCTest
@testable import VaultdayCore

/// The Security step: the modes are drawn, every workspace stays Off, and the fields the app document reserved are the ones written.
@MainActor
final class SecurityStepModelTests: XCTestCase {
	private func twoWorkspaces(_ harness: OnboardingHarness) throws -> [WorkspaceEntry] {
		let workspaces = WorkspacesModel(session: harness.session)
		let work = try XCTUnwrap(workspaces.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Work"), sphere: .work, name: "Work"))
		let life = try XCTUnwrap(workspaces.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Life"), sphere: .life, name: "Life"))
		return [work, life]
	}

	func testTheThreeLevelsAreDrawnAndOffIsTheOnlyOneAvailable() {
		XCTAssertEqual(SecurityLevel.allCases.map(\.title), ["Off", "Enhanced", "Strict"])
		XCTAssertTrue(SecurityLevel.allCases.allSatisfy { !$0.points.isEmpty })
		XCTAssertFalse(SecurityStepModel.areLevelsAvailable)
		XCTAssertFalse(SecurityStepModel.plannedNote.hasSuffix("."))
	}

	func testEveryWorkspaceIsRecordedOffInTheAppDocument() throws {
		let harness = try OnboardingHarness(self)
		let entries = try twoWorkspaces(harness)
		let security = SecurityStepModel(session: harness.session)
		security.refresh(workspaces: entries)
		for entry in entries {
			XCTAssertEqual(security.level(of: entry.id), .off)
			let stored = harness.session.appState.security(workspace: entry.id)
			XCTAssertEqual(stored.mode, .off)
			XCTAssertFalse(stored.alwaysAskBeforeSending)
		}
		XCTAssertNil(security.errorMessage)
	}

	func testALevelAboveOffIsRefusedAndNothingIsWritten() throws {
		let harness = try OnboardingHarness(self)
		let entries = try twoWorkspaces(harness)
		let security = SecurityStepModel(session: harness.session)
		security.refresh(workspaces: entries)
		XCTAssertFalse(security.setLevel(.strict, workspace: entries[0].id))
		XCTAssertFalse(security.setLevel(.enhanced, workspace: entries[1].id))
		XCTAssertEqual(security.errorMessage, SecurityStepModel.plannedNote)
		XCTAssertEqual(security.level(of: entries[0].id), .off)
		XCTAssertEqual(harness.session.appState.security(workspace: entries[0].id).mode, .off)
		XCTAssertTrue(security.setLevel(.off, workspace: entries[0].id))
		XCTAssertNil(security.errorMessage)
	}

	func testAnotherDevicesRecordedLevelIsShownNotOverwritten() throws {
		let harness = try OnboardingHarness(self)
		let entries = try twoWorkspaces(harness)
		try harness.session.edit { try $0.setSecurity(workspace: entries[0].id, security: WorkspaceSecurityView(mode: .strict, alwaysAskBeforeSending: false)) }
		let security = SecurityStepModel(session: harness.session)
		security.refresh(workspaces: entries)
		XCTAssertEqual(security.level(of: entries[0].id), .strict)
		XCTAssertEqual(harness.session.appState.security(workspace: entries[0].id).mode, .strict)
	}
}
