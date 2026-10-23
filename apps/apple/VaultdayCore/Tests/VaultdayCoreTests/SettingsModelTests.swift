import Foundation
import XCTest
@testable import VaultdayCore

/// The Settings window's app-level choices: each one persists through the core.
@MainActor
final class SettingsModelTests: XCTestCase {
	func testKeyboardSchemeHoverSelectsAndShortcutCapsPersist() throws {
		let harness = try OnboardingHarness(self)
		let settings = SettingsModel(session: harness.session)
		XCTAssertEqual(settings.keyboardScheme, .standard)
		XCTAssertTrue(settings.hoverSelects)
		XCTAssertEqual(settings.shortcutCaps, .animated)

		settings.setKeyboardScheme(.vimMotions)
		settings.setHoverSelects(false)
		settings.setShortcutCaps(.instant)
		XCTAssertNil(settings.errorMessage)

		let reopened = SettingsModel(session: try harness.relaunched().session)
		XCTAssertEqual(reopened.keyboardScheme, .vimMotions)
		XCTAssertFalse(reopened.hoverSelects)
		XCTAssertEqual(reopened.shortcutCaps, .instant)
	}

	func testTheDeviceIdIsTheSessionsAndCopiesAsIs() throws {
		let harness = try OnboardingHarness(self)
		var copied: [String] = []
		let settings = SettingsModel(session: harness.session, copyToPasteboard: { copied.append($0) })
		XCTAssertEqual(settings.deviceId, harness.session.deviceId)
		XCTAssertFalse(settings.deviceId.isEmpty)
		settings.copyDeviceId()
		XCTAssertEqual(copied, [harness.session.deviceId])
		XCTAssertFalse(SettingsModel.deviceIdNote.hasSuffix("."))
		let reopened = SettingsModel(session: try harness.relaunched().session)
		XCTAssertEqual(reopened.deviceId, settings.deviceId, "the id is the device's, not the run's")
	}

	func testTheDayChoicesTheWindowShowsAreTheOnesTheStepWrote() throws {
		let harness = try OnboardingHarness(self)
		let step = RhythmModel(session: harness.session)
		step.setBreakPattern(.none)
		step.setTime("07:50", of: .planning)
		step.setWorkloadThreshold(minutes: 360)
		let inSettings = RhythmModel(session: try harness.relaunched().session)
		XCTAssertEqual(inSettings.breakPattern, BreakPatternSetting.none)
		XCTAssertEqual(inSettings.time(of: .planning).text, "07:50")
		XCTAssertEqual(inSettings.workloadThresholdMinutes, 360)
		inSettings.setRolloverPosition(.bottom)
		inSettings.setNewTaskPosition(.top)
		XCTAssertEqual(RhythmModel(session: try harness.relaunched().session).rolloverPosition, .bottom)
		XCTAssertEqual(RhythmModel(session: try harness.relaunched().session).newTaskPosition, .top)
	}

	func testTheConfigureModelAWorkspaceOpensWithIsTheOneTheStepsUse() throws {
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Studio"), sphere: .work, name: "Studio"))
		let configure = WorkspaceConfigureModel(root: entry.rootPath)
		XCTAssertTrue(configure.isLoaded)
		XCTAssertEqual(configure.name, "Studio")
		XCTAssertFalse(configure.hasChanges)
		configure.hoursPerDay = "6"
		XCTAssertTrue(configure.hasChanges)
	}
}
