import Foundation
import XCTest
@testable import VaultdayCore

/// The Configure sheet: a round trip of the mock vault's settings.json, the
/// core's validation messages, and the one-time Workspace.md prefill.
@MainActor
final class ConfigureModelTests: XCTestCase {
	private func settingsText(_ folder: URL) throws -> String {
		try String(contentsOf: folder.appending(path: ".vaultday/settings.json"), encoding: .utf8)
	}

	func testTheSheetLoadsEveryFieldOfTheMockVault() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let model = WorkspaceConfigureModel(root: vault.path)
		XCTAssertTrue(model.isLoaded)
		XCTAssertEqual(model.name, "Mock Studio")
		XCTAssertEqual(model.sphere, .work)
		XCTAssertEqual(model.accent, "teal")
		XCTAssertEqual(model.ticketPattern, "(WEB|ACME)-\\d+")
		XCTAssertEqual(model.ticketLink, "https://tracker.example.test/browse/{ticket}")
		XCTAssertEqual(model.hosts, "tracker.example.test")
		XCTAssertEqual(model.browser, "default")
		XCTAssertEqual(model.hoursPerDay, "8")
		XCTAssertEqual(model.logFolder, "Log")
		XCTAssertEqual(model.storyTemplate, "_templates/story.md")
		XCTAssertFalse(model.hasChanges)
		XCTAssertEqual(model.settingsPath, "\(vault.path)/.vaultday/settings.json")
	}

	func testSavingRoundTripsTheMockVaultWithoutLosingUnknownKeys() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let model = WorkspaceConfigureModel(root: vault.path)
		XCTAssertTrue(model.save(), "an unchanged sheet saves what the file already says")
		var json = try TestFixtures.settingsJSON(in: vault)
		let future = try XCTUnwrap(json["x_future_setting"] as? [String: Any])
		XCTAssertEqual(future["kept"] as? Bool, true)

		model.name = "Mock Studio East"
		model.hosts = "tracker.example.test, wiki.example.test"
		model.hoursPerDay = "7.5"
		model.accent = "plum"
		XCTAssertTrue(model.hasChanges)
		XCTAssertTrue(model.save())
		json = try TestFixtures.settingsJSON(in: vault)
		XCTAssertEqual(json["name"] as? String, "Mock Studio East")
		XCTAssertEqual(json["hosts"] as? [String], ["tracker.example.test", "wiki.example.test"])
		XCTAssertEqual(json["hours_per_day"] as? Double, 7.5)
		XCTAssertEqual(json["accent"] as? String, "plum")
		XCTAssertEqual((json["x_future_setting"] as? [String: Any])?["note"] as? String, "an unknown key a newer build wrote; this build preserves it")
		XCTAssertEqual(json["ticket_pattern"] as? String, "(WEB|ACME)-\\d+", "untouched fields stay as written")
		XCTAssertFalse(model.hasChanges)

		let reopened = WorkspaceConfigureModel(root: vault.path)
		XCTAssertEqual(reopened.name, "Mock Studio East")
		XCTAssertEqual(reopened.hoursPerDay, "7.5")
	}

	func testClearingAnOptionalFieldRemovesItFromTheFile() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let model = WorkspaceConfigureModel(root: vault.path)
		model.storyTemplate = ""
		model.browser = "  "
		XCTAssertTrue(model.save())
		let json = try TestFixtures.settingsJSON(in: vault)
		XCTAssertNil(json["story_template"])
		XCTAssertNil(json["browser"])
	}

	func testTheCoresMessagesSurfacePerFieldAndNothingIsWritten() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let before = try settingsText(vault)
		let model = WorkspaceConfigureModel(root: vault.path)
		model.name = " "
		model.hoursPerDay = "lots"
		model.ticketPattern = "(unclosed"
		model.ticketLink = "https://tracker.example.test/browse/"
		model.hosts = "https://tracker.example.test"
		model.logFolder = "../elsewhere"

		XCTAssertFalse(model.save())
		XCTAssertEqual(Set(model.issues.keys), ["name", "hours_per_day", "ticket_pattern", "ticket_url", "hosts", "log_folder"])
		XCTAssertEqual(model.issues["name"], "Give the workspace a name")
		XCTAssertEqual(model.issues["hours_per_day"], "Working hours must be above 0 and at most 24")
		XCTAssertTrue(try XCTUnwrap(model.issues["ticket_pattern"]).hasPrefix("The ticket pattern is not a valid regular expression"))
		XCTAssertTrue(try XCTUnwrap(model.issues["ticket_url"]).contains("{ticket}"))
		XCTAssertEqual(try settingsText(vault), before)

		model.reload()
		XCTAssertTrue(model.issues.isEmpty)
		XCTAssertEqual(model.name, "Mock Studio")
	}

	func testAFolderWithoutAManifestReportsItInsteadOfCrashing() throws {
		let folder = try TestFixtures.emptyFolder(self, named: "plain")
		let model = WorkspaceConfigureModel(root: folder.path)
		XCTAssertFalse(model.isLoaded)
		XCTAssertNotNil(model.errorMessage)
		XCTAssertFalse(model.save())
	}

	func testWorkspaceMdPrefillsTheFirstManifestOnceAndIsNeverReadAgain() throws {
		let folder = try TestFixtures.emptyFolder(self, named: "Studio")
		let note = folder.appending(path: "Workspace.md")
		try Data("---\nhosts:\n  - tracker.example.test\nhours_per_day: 6\nticket_pattern: \"WEB-\\\\d+\"\n---\n# Studio From Note\n".utf8).write(to: note)
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)

		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: folder, sphere: .work, name: ""))
		XCTAssertEqual(entry.name, "Studio From Note")
		let model = WorkspaceConfigureModel(root: folder.path)
		XCTAssertEqual(model.hosts, "tracker.example.test")
		XCTAssertEqual(model.hoursPerDay, "6")
		XCTAssertEqual(model.ticketPattern, "WEB-\\d+")

		// The note changes and then becomes unreadable: configuring, saving,
		// reloading and listing all keep working from settings.json alone.
		try Data("---\nhours_per_day: 3\n---\n# Changed\n".utf8).write(to: note)
		try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: note.path)
		addTeardownBlock { try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: note.path) }
		model.name = "Studio Renamed"
		XCTAssertTrue(model.save())
		workspaces.reload()
		XCTAssertEqual(workspaces.entries.map(\.name), ["Studio Renamed"])
		let reopened = WorkspaceConfigureModel(root: folder.path)
		XCTAssertEqual(reopened.hoursPerDay, "6")
		XCTAssertNil(workspaces.addWorkspace(folder: folder, sphere: .work, name: "Again"), "an existing workspace is not created again")
		XCTAssertEqual(WorkspaceConfigureModel(root: folder.path).hoursPerDay, "6")
	}
}
