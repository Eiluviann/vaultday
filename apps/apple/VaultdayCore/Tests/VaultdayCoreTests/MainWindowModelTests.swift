import Foundation
import XCTest
@testable import VaultdayCore

/// The one main window: setup or Home at its root, Settings and the sync
/// check as sheets over either, ⌘, through the keymap.
@MainActor
final class MainWindowModelTests: XCTestCase {
	func testTheWindowDrawsNothingUntilTheLaunchDecides() {
		XCTAssertEqual(MainWindowModel().root, .undecided, "no empty Home before setup")
		let failed = MainWindowModel()
		failed.launchFailed()
		XCTAssertEqual(failed.root, .home, "Home's Today column says why")
	}

	func testAChildSheetHoldsOffSettingsAndTheSyncCheckWithNoStateChange() {
		let window = MainWindowModel()
		window.launched(needsSetup: false)
		let closeOut = UUID()
		window.childSheetOpened(closeOut)
		window.toggleSettings()
		XCTAssertNil(window.panel, "⌘, is ignored while the close-out sheet is open")
		window.showSyncCheck()
		XCTAssertNil(window.panel)
		window.childSheetClosed(closeOut)
		XCTAssertFalse(window.hasChildSheet)
		window.toggleSettings()
		XCTAssertEqual(window.panel, .settings)

		let configure = UUID()
		window.childSheetOpened(configure)
		window.childSheetOpened(configure)
		window.toggleSettings()
		window.showSyncCheck()
		XCTAssertEqual(window.panel, .settings, "Settings stays over its own Configure sheet")
		window.childSheetClosed(configure)
		window.toggleSettings()
		XCTAssertNil(window.panel)
	}

	func testAFreshUserLandsInSetupAndAReturningOneInHome() {
		let fresh = MainWindowModel()
		fresh.launched(needsSetup: true)
		XCTAssertEqual(fresh.root, .setup)

		let returning = MainWindowModel()
		returning.launched(needsSetup: false)
		XCTAssertEqual(returning.root, .home)
		XCTAssertNil(returning.panel)
	}

	func testSetupHandsTheWindowToHomeAndTheMenuBringsItBack() {
		let window = MainWindowModel()
		window.launched(needsSetup: true)
		window.showHome()
		XCTAssertEqual(window.root, .home)

		window.toggleSettings()
		window.showSetup()
		XCTAssertEqual(window.root, .setup)
		XCTAssertNil(window.panel, "setup fills the window with no sheet over it")
	}

	func testSettingsOpensOverEitherRootAndTheSameKeyClosesIt() {
		for needsSetup in [true, false] {
			let window = MainWindowModel()
			window.launched(needsSetup: needsSetup)
			window.toggleSettings()
			XCTAssertEqual(window.panel, .settings)
			XCTAssertEqual(window.root, needsSetup ? .setup : .home, "Settings leaves the root alone")
			window.toggleSettings()
			XCTAssertNil(window.panel)
		}
	}

	func testSettingsTakesThePlaceOfTheSyncCheckAndEscClosesEither() {
		let window = MainWindowModel()
		window.showSyncCheck()
		window.toggleSettings()
		XCTAssertEqual(window.panel, .settings)
		window.closePanel()
		XCTAssertNil(window.panel)
		window.showSyncCheck()
		window.closePanel()
		XCTAssertNil(window.panel)
	}

	func testCommandCommaOpensAndClosesSettingsThroughTheKeymapInBothSchemes() throws {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		let commandComma = try XCTUnwrap(KeyStroke(notation: "cmd+,"))
		for scheme in KeyScheme.allCases {
			XCTAssertEqual(keymap.singleStroke(for: .toggleSettings, scheme: scheme), commandComma, "the app menu's Settings… key under \(scheme)")
			let window = MainWindowModel()
			let session = KeyboardSession(dispatcher: KeyDispatcher(keymap: keymap, scheme: scheme), focus: PaneFocusModel(scheme: scheme), register: PlanRegister(plan: FakePlan()) { _ in })
			session.onAction = { if $0 == .toggleSettings { window.toggleSettings() } }
			session.handle(commandComma)
			XCTAssertEqual(window.panel, .settings, "\(scheme)")
			session.handle(commandComma)
			XCTAssertNil(window.panel, "\(scheme)")
		}
	}

	func testTheMainWindowCountsAsFrontmostUnderItsSheetsAndPopovers() {
		XCTAssertTrue(MainWindowModel.isMainWindow(lineage: ["home-AppWindow-1"]))
		XCTAssertTrue(MainWindowModel.isMainWindow(lineage: [nil, "home-AppWindow-1"]), "the Settings sheet over Home")
		XCTAssertTrue(MainWindowModel.isMainWindow(lineage: [nil, nil, "home-AppWindow-1"]), "a popover in the Settings sheet")
		XCTAssertFalse(MainWindowModel.isMainWindow(lineage: [nil]), "the mini tracker or the menu-bar extra")
		XCTAssertFalse(MainWindowModel.isMainWindow(lineage: []), "no key window")
	}
}
