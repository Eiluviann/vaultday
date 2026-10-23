import Foundation
import XCTest
@testable import VaultdayCore

/// The Workspaces and Folders steps over the mock vault and temporary folders.
@MainActor
final class WorkspacesModelTests: XCTestCase {
	private let forbiddenKeys = ["security", "tokens", "rates", "models", "integrations"]
	private let knownKeys: Set<String> = [
		"schema_version", "name", "kind", "accent", "ticket_pattern", "ticket_url", "hosts", "browser",
		"hours_per_day", "log_folder", "story_template", "features",
	]

	func testAddingTheMockVaultListsItAndARelaunchRestoresIt() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: vault, sphere: .life))
		XCTAssertEqual(entry.name, "Mock Studio")
		XCTAssertEqual(entry.sphere, .work, "the manifest's own kind wins over the sphere the Add key sat in")
		XCTAssertTrue(entry.id.hasPrefix("ws-"))
		XCTAssertEqual(entry.folders.map(\.isWorkspaceRoot), [true])
		XCTAssertEqual(entry.folders.first?.kindLabel, "Obsidian vault")

		let relaunched = WorkspacesModel(session: try harness.relaunched().session)
		XCTAssertEqual(relaunched.entries, workspaces.entries)
		XCTAssertEqual(relaunched.problems, [])
	}

	func testTheStepsAddKeyTakesTheNotesNameAndSkipsTheValuesTheFormRefuses() throws {
		let folder = try TestFixtures.emptyFolder(self, named: "Studio")
		try Data("---\nhosts:\n  - https://tracker.example.test\nhours_per_day: 6\n---\n# Studio From Note\n".utf8).write(to: folder.appending(path: "Workspace.md"))
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)

		let entry = workspaces.addWorkspace(folder: folder, sphere: .work)
		XCTAssertNil(workspaces.message)
		XCTAssertEqual(entry?.name, "Studio From Note")
		let json = try TestFixtures.settingsJSON(in: folder)
		XCTAssertNil(json["hosts"], "the URL is left out, not written as a host")
		XCTAssertEqual(json["hours_per_day"] as? Double, 6)
	}

	func testANewFolderBecomesAWorkspaceWithAManifestThatMatchesTheSchema() throws {
		let folder = try TestFixtures.emptyFolder(self, named: "Freelancing")
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: folder, sphere: .work, name: "Freelancing"))
		XCTAssertEqual(entry.sphere, .work)

		let json = try TestFixtures.settingsJSON(in: folder)
		XCTAssertEqual(json["name"] as? String, "Freelancing")
		XCTAssertEqual(json["kind"] as? String, "work")
		XCTAssertEqual(json["schema_version"] as? Int, 1)
		XCTAssertTrue(Set(json.keys).isSubset(of: knownKeys), "unexpected keys: \(Set(json.keys).subtracting(knownKeys))")
		for key in forbiddenKeys { XCTAssertNil(json[key]) }
	}

	func testTheSameFolderTwiceAndNestedClientWorkspacesAreRefusedWithAMessage() throws {
		let outer = try TestFixtures.emptyFolder(self, named: "clients")
		let inner = outer.appending(path: "acme", directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: inner, withIntermediateDirectories: true)
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)
		XCTAssertNotNil(workspaces.addWorkspace(folder: outer, sphere: .work, name: "Clients"))

		XCTAssertNil(workspaces.addWorkspace(folder: outer, sphere: .work, name: "Again"))
		XCTAssertEqual(workspaces.message, "\(outer.resolvingSymlinksInPath().path) is already a folder of Clients")

		XCTAssertNil(workspaces.addWorkspace(folder: inner, sphere: .work, name: "Acme"))
		XCTAssertTrue(try XCTUnwrap(workspaces.message).contains("overlap"))
		XCTAssertFalse(FileManager.default.fileExists(atPath: inner.appending(path: ".vaultday").path), "a refused folder gets no manifest")
		XCTAssertEqual(workspaces.entries.count, 1)

		XCTAssertNotNil(workspaces.addWorkspace(folder: inner, sphere: .life, name: "Acme life"), "a life workspace may nest inside client work")
	}

	func testFurtherFoldersAreGrantedListedAndRemovedWithoutTouchingTheNotes() throws {
		let root = try TestFixtures.emptyFolder(self, named: "Freelancing")
		let docs = try TestFixtures.emptyFolder(self, named: "docs")
		try Data("# Doc\n".utf8).write(to: docs.appending(path: "a.md"))
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: root, sphere: .work, name: "Freelancing"))

		XCTAssertTrue(workspaces.addFolder(docs, to: entry.id))
		let updated = try XCTUnwrap(workspaces.entry(id: entry.id))
		XCTAssertEqual(updated.folders.map(\.name), ["Freelancing", "docs"])
		XCTAssertEqual(updated.folders.map(\.kindLabel), ["Vault folder", "Markdown folder"])
		XCTAssertFalse(workspaces.addFolder(docs, to: entry.id))
		XCTAssertTrue(try XCTUnwrap(workspaces.message).contains("already a folder of Freelancing"))

		let extra = try XCTUnwrap(updated.folders.last)
		workspaces.removeFolder(extra.id, from: entry.id)
		XCTAssertEqual(try XCTUnwrap(workspaces.entry(id: entry.id)).folders.map(\.name), ["Freelancing"])
		XCTAssertTrue(FileManager.default.fileExists(atPath: docs.appending(path: "a.md").path))
		let own = try XCTUnwrap(updated.folders.first)
		workspaces.removeFolder(own.id, from: entry.id)
		XCTAssertEqual(try XCTUnwrap(workspaces.entry(id: entry.id)).folders.count, 1, "the workspace's own folder goes with the workspace")
	}

	func testANewFolderIsCreatedThenAdded() throws {
		let root = try TestFixtures.emptyFolder(self, named: "Personal")
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: root, sphere: .life, name: "Personal"))
		let fresh = root.deletingLastPathComponent().appending(path: "Recipes", directoryHint: .isDirectory)
		XCTAssertTrue(workspaces.addNewFolder(fresh, to: entry.id))
		XCTAssertTrue(FileManager.default.fileExists(atPath: fresh.path))
		XCTAssertEqual(try XCTUnwrap(workspaces.entry(id: entry.id)).folders.count, 2)
	}

	func testRemovingAWorkspaceDestroysItsKeyArchivesItsSyncFolderAndAllowsAddingItAgain() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		var changes = 0
		workspaces.onChange = { changes += 1 }
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: vault, sphere: .work))
		// Start the workspace's app state so there is a key and a sync folder to remove.
		let opened = WorkspaceModel(session: harness.session)
		opened.restoreAtLaunch()
		XCTAssertEqual(opened.phase, .open)
		opened.close()
		XCTAssertTrue(harness.keychain.storedItems.keys.contains { $0.account == entry.id })

		workspaces.removeWorkspace(id: entry.id)

		XCTAssertEqual(workspaces.entries, [])
		XCTAssertFalse(harness.keychain.storedItems.keys.contains { $0.account == entry.id })
		let vaultday = try FileManager.default.contentsOfDirectory(atPath: vault.appending(path: ".vaultday").path)
		XCTAssertTrue(vaultday.contains { $0.hasPrefix("sync-removed-") })
		XCTAssertGreaterThanOrEqual(changes, 2)
		XCTAssertNotNil(workspaces.addWorkspace(folder: vault, sphere: .work))
	}

	func testAWorkspaceFolderThatHoldsAnotherDevicesStateIsRefusedUntilItArrives() throws {
		// The folder holds app state from another device but not its workspace.id yet.
		let vault = try TestFixtures.copyOfMockVault(self)
		let other = try OnboardingHarness(self)
		let first = WorkspacesModel(session: other.session)
		XCTAssertNotNil(first.addWorkspace(folder: vault, sphere: .work))
		let opened = WorkspaceModel(session: other.session)
		opened.restoreAtLaunch()
		opened.close()
		try FileManager.default.removeItem(at: vault.appending(path: ".vaultday/sync/workspace.id"))

		let second = WorkspacesModel(session: try OnboardingHarness(self).session)
		XCTAssertNil(second.addWorkspace(folder: vault, sphere: .work))
		XCTAssertNotNil(second.message)
		XCTAssertEqual(second.entries, [])
	}
}
