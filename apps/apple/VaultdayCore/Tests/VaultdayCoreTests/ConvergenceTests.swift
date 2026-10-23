import Foundation
import XCTest
@testable import VaultdayCore

/// The two-device convergence test of PR 4, run through the Swift ports:
/// two "Macs" whose ``KeychainSecretStore``s share the synchronisable items
/// of one ``InMemoryKeychainBackend`` (iCloud Keychain's role) and keep their
/// own device-only items, both reaching the same workspace folder through
/// ``CoordinatedSyncFiles`` (the synced folder's role). The real two-Mac run
/// over iCloud is the manual test in the PR.
final class ConvergenceTests: XCTestCase {
	private struct Mac {
		let store: KeychainSecretStore
		let files: CoordinatedSyncFiles
		let deviceId: String
	}

	private func mac(_ backend: InMemoryKeychainBackend, root: URL) throws -> Mac {
		let store = KeychainSecretStore(backend: backend, accessGroup: "group.com.eiluviann.vaultday", wrapper: ECIESKeyWrapper.software())
		return Mac(store: store, files: CoordinatedSyncFiles(roots: [root]), deviceId: try loadDeviceId(secrets: store))
	}

	private func create(_ mac: Mac, _ root: URL) throws -> WorkspaceStateHandle {
		try createWorkspaceState(files: mac.files, workspaceRoot: root.path, workspaceId: "mock-studio", deviceId: mac.deviceId, secrets: mac.store)
	}

	private func open(_ mac: Mac, _ root: URL) throws -> WorkspaceStateHandle {
		try openWorkspaceState(files: mac.files, workspaceRoot: root.path, workspaceId: "mock-studio", deviceId: mac.deviceId, secrets: mac.store)
	}

	func testTwoMacsConvergeThroughTheKeychainStoreAndAConflictCopy() throws {
		let root = try TestFixtures.copyOfMockVault(self)
		let backend = InMemoryKeychainBackend()
		let first = try mac(backend, root: root)
		let second = try mac(backend.forAnotherDevice(), root: root)
		XCTAssertNotEqual(first.deviceId, second.deviceId)

		let firstState = try create(first, root)
		try firstState.setDayOrder(date: "2026-10-22", taskIds: ["task-1", "task-2"])
		let written = try XCTUnwrap(try firstState.flush())

		// A sync client's conflict copy of the same file is read as an ordinary change.
		let conflictCopy = URL(fileURLWithPath: written).deletingPathExtension().path + " 2.bin"
		try FileManager.default.copyItem(atPath: written, toPath: conflictCopy)

		let secondState = try open(second, root)
		XCTAssertEqual(try secondState.dayOrder(date: "2026-10-22"), ["task-1", "task-2"])
		XCTAssertEqual(secondState.lastPull().pending, [])

		try secondState.setDayOrder(date: "2026-10-22", taskIds: ["task-2", "task-1"])
		_ = try secondState.flush()
		XCTAssertGreaterThan(try firstState.pull().applied, 0)

		XCTAssertEqual(try firstState.dayOrder(date: "2026-10-22"), ["task-2", "task-1"])
		XCTAssertEqual(firstState.heads(), secondState.heads(), "both documents converged")
	}

	func testAMacWithoutTheKeyWaitsInsteadOfMintingASecondOne() throws {
		let root = try TestFixtures.copyOfMockVault(self)
		let first = try mac(InMemoryKeychainBackend(), root: root)
		let firstState = try create(first, root)
		_ = try firstState.flush()

		// A Mac outside the synchronised set: iCloud Keychain has not delivered the key.
		let stranger = try mac(InMemoryKeychainBackend(), root: root)
		XCTAssertThrowsError(try open(stranger, root)) {
			guard case .KeyNotArrived = $0 as? CoreError else { return XCTFail("\($0)") }
		}
		XCTAssertThrowsError(try create(stranger, root)) {
			guard case .Conflict = $0 as? CoreError else { return XCTFail("\($0)") }
		}
		XCTAssertEqual(try stranger.store.list(kind: .workspaceDataKey), [])
	}

	func testRemovingTheWorkspaceLocksEveryMacOut() throws {
		let root = try TestFixtures.copyOfMockVault(self)
		let backend = InMemoryKeychainBackend()
		let first = try mac(backend, root: root)
		let second = try mac(backend.forAnotherDevice(), root: root)
		_ = try create(first, root).flush()

		try deleteWorkspaceKeys(secrets: second.store, workspaceId: "mock-studio")

		XCTAssertThrowsError(try open(first, root)) {
			guard case .KeyNotArrived = $0 as? CoreError else { return XCTFail("\($0)") }
		}
	}
}
