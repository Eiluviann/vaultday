import Foundation
import XCTest
@testable import VaultdayCore

/// The models over an in-memory environment: a folder is granted, the
/// workspace opens from the core, the grant survives a relaunch, settings
/// persist in the signed app document.
@MainActor
final class ModelTests: XCTestCase {
	private func environment(_ container: URL, keychain: InMemoryKeychainBackend) throws -> CoreEnvironment {
		try CoreEnvironment.inMemory(container: container, keychain: keychain)
	}

	func testGrantingTheMockVaultOpensItAndARelaunchRestoresIt() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let container = try TestFixtures.temporaryFolder(self, "Group")
		let keychain = InMemoryKeychainBackend()

		let session = try CoreSession.open(try environment(container, keychain: keychain))
		let workspace = WorkspaceModel(session: session)
		XCTAssertEqual(workspace.phase, .noFolder)
		workspace.grantFolder(vault)
		XCTAssertEqual(workspace.phase, .open)
		XCTAssertEqual(workspace.name, "Mock Studio")
		XCTAssertTrue(workspace.workspaceId.hasPrefix("ws-"), workspace.workspaceId)
		XCTAssertEqual(workspace.appStateStatus, "App state started on this Mac")

		let plan = DayPlanModel()
		plan.load(from: workspace, date: "2026-10-22")
		XCTAssertEqual(plan.stories.map(\.number), ["WEB-142", "WEB-157", "ACME-9"])
		let sessions = SessionModel()
		sessions.load(from: workspace)
		XCTAssertEqual(sessions.sessions, [])
		XCTAssertNil(sessions.openSession)

		let relaunched = WorkspaceModel(session: try CoreSession.open(try environment(container, keychain: keychain)))
		relaunched.restoreAtLaunch()
		XCTAssertEqual(relaunched.phase, .open)
		XCTAssertEqual(relaunched.rootPath, workspace.rootPath)
		XCTAssertEqual(relaunched.workspaceId, workspace.workspaceId)
		XCTAssertTrue(relaunched.appStateStatus.hasPrefix("App state up to date"), relaunched.appStateStatus)
		XCTAssertEqual(relaunched.grantProblems, [])
	}

	func testTheSyncProbeConvergesBetweenTwoMacs() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let keychain = InMemoryKeychainBackend()
		let first = WorkspaceModel(session: try CoreSession.open(try environment(try TestFixtures.temporaryFolder(self, "GroupA"), keychain: keychain)))
		let second = WorkspaceModel(session: try CoreSession.open(try environment(try TestFixtures.temporaryFolder(self, "GroupB"), keychain: keychain.forAnotherDevice())))

		first.grantFolder(vault)
		first.writeSyncProbe()
		second.grantFolder(vault)
		second.writeSyncProbe()
		first.pull()

		let firstSummary = try XCTUnwrap(first.syncSummary)
		let secondSummary = try XCTUnwrap(second.syncSummary)
		XCTAssertNotEqual(firstSummary.deviceId, secondSummary.deviceId)
		XCTAssertEqual(firstSummary.probes.count, 2)
		XCTAssertEqual(firstSummary.headsDigest, secondSummary.headsDigest)
	}

	func testCalendarPicksPersistPerWorkspace() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let container = try TestFixtures.temporaryFolder(self, "Group")
		let keychain = InMemoryKeychainBackend()
		let workspace = WorkspaceModel(session: try CoreSession.open(try environment(container, keychain: keychain)))
		workspace.grantFolder(vault)
		workspace.setCalendar(id: "cal-work", title: "Work", isPicked: true)
		workspace.setCalendar(id: "cal-home", title: "Home", isPicked: true)
		workspace.setCalendar(id: "cal-home", title: "Home", isPicked: false)
		XCTAssertEqual(workspace.pickedCalendarIds, ["cal-work"])

		let relaunched = WorkspaceModel(session: try CoreSession.open(try environment(container, keychain: keychain)))
		relaunched.restoreAtLaunch()
		XCTAssertEqual(relaunched.pickedCalendarIds, ["cal-work"], "the folder connection is not a calendar pick")
	}

	func testAGrantThatNoLongerResolvesIsListed() throws {
		let parent = try TestFixtures.temporaryFolder(self, "vaults")
		let vault = parent.appending(path: "mock-vault", directoryHint: .isDirectory)
		try FileManager.default.copyItem(at: TestFixtures.mockVault, to: vault)
		let container = try TestFixtures.temporaryFolder(self, "Group")
		let keychain = InMemoryKeychainBackend()
		WorkspaceModel(session: try CoreSession.open(try environment(container, keychain: keychain))).grantFolder(vault)
		try FileManager.default.removeItem(at: vault)

		let relaunched = WorkspaceModel(session: try CoreSession.open(try environment(container, keychain: keychain)))
		relaunched.restoreAtLaunch()
		XCTAssertEqual(relaunched.phase, .noFolder)
		XCTAssertEqual(relaunched.grantProblems.count, 1)
	}

	func testRemovingTheWorkspaceDestroysItsKeyAndGrantAndTheFolderCanBeAddedAgain() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let container = try TestFixtures.temporaryFolder(self, "Group")
		let keychain = InMemoryKeychainBackend()
		let session = try CoreSession.open(try environment(container, keychain: keychain))
		let workspace = WorkspaceModel(session: session)
		workspace.grantFolder(vault)
		let removedId = workspace.workspaceId
		XCTAssertTrue(keychain.storedItems.keys.contains { $0.account == removedId })

		workspace.removeWorkspace()

		XCTAssertEqual(workspace.phase, .noFolder)
		XCTAssertFalse(keychain.storedItems.keys.contains { $0.account == removedId })
		XCTAssertEqual(SettingsModel(session: session).connections, [])
		let vaultday = try FileManager.default.contentsOfDirectory(atPath: vault.appending(path: ".vaultday").path)
		XCTAssertFalse(vaultday.contains("sync"))
		XCTAssertTrue(vaultday.contains { $0.hasPrefix("sync-removed-") })

		workspace.grantFolder(vault)
		XCTAssertEqual(workspace.appStateStatus, "App state started on this Mac")
		XCTAssertNotEqual(workspace.workspaceId, removedId)
	}

	func testTwoWorkspacesOfOneNameKeepTheirKeysApartAndARenameKeepsTheId() throws {
		let first = try TestFixtures.copyOfMockVault(self)
		let second = try TestFixtures.copyOfMockVault(self)
		let keychain = InMemoryKeychainBackend()
		let session = try CoreSession.open(try environment(try TestFixtures.temporaryFolder(self, "Group"), keychain: keychain))
		let one = WorkspaceModel(session: session)
		one.grantFolder(first)
		let other = WorkspaceModel(session: session)
		other.grantFolder(second)
		XCTAssertEqual(one.name, other.name)
		XCTAssertNotEqual(one.workspaceId, other.workspaceId)

		let manifest = first.appending(path: ".vaultday/settings.json")
		let renamed = try String(contentsOf: manifest, encoding: .utf8).replacingOccurrences(of: "Mock Studio", with: "Renamed Studio")
		try renamed.write(to: manifest, atomically: true, encoding: .utf8)
		let reopened = WorkspaceModel(session: session)
		reopened.grantFolder(first)
		XCTAssertEqual(reopened.name, "Renamed Studio")
		XCTAssertEqual(reopened.workspaceId, one.workspaceId)
		XCTAssertTrue(reopened.appStateStatus.hasPrefix("App state up to date"), reopened.appStateStatus)
	}

	func testAFolderThatIsNotAWorkspaceFails() throws {
		let folder = try TestFixtures.temporaryFolder(self, "plain")
		let workspace = WorkspaceModel(session: try CoreSession.open(try environment(try TestFixtures.temporaryFolder(self, "Group"), keychain: InMemoryKeychainBackend())))
		workspace.grantFolder(folder)
		guard case let .failed(message) = workspace.phase else { return XCTFail("\(workspace.phase)") }
		XCTAssertTrue(message.contains("not a workspace"), message)
	}

	func testSettingsPersistAcrossRelaunches() throws {
		let container = try TestFixtures.temporaryFolder(self, "Group")
		let keychain = InMemoryKeychainBackend()
		let settings = SettingsModel(session: try CoreSession.open(try environment(container, keychain: keychain)))
		XCTAssertEqual(settings.keyboardScheme, .standard)
		settings.setKeyboardScheme(.vimMotions)
		settings.setHoverSelects(false)
		XCTAssertNil(settings.errorMessage)

		let reopened = SettingsModel(session: try CoreSession.open(try environment(container, keychain: keychain)))
		XCTAssertEqual(reopened.keyboardScheme, .vimMotions)
		XCTAssertFalse(reopened.hoverSelects)
		XCTAssertEqual(reopened.refusedSnapshots, [])
	}

	func testATamperedSnapshotIsRefusedAndReported() throws {
		let container = try TestFixtures.temporaryFolder(self, "Group")
		let keychain = InMemoryKeychainBackend()
		let session = try CoreSession.open(try environment(container, keychain: keychain))
		SettingsModel(session: session).setKeyboardScheme(.vimMotions)
		let snapshot = AppGroupLayout(container: container).appStateFolder.appending(path: "\(session.deviceId).bin")
		var bytes = try Data(contentsOf: snapshot)
		bytes[bytes.count / 2] ^= 0xFF
		try bytes.write(to: snapshot)

		let reopened = SettingsModel(session: try CoreSession.open(try environment(container, keychain: keychain)))
		XCTAssertEqual(reopened.refusedSnapshots.count, 1)
		XCTAssertEqual(reopened.keyboardScheme, .standard, "nothing of a refused snapshot is merged")
	}
}
