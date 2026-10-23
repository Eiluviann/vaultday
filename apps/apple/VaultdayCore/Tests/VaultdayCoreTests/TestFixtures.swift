import Foundation
import XCTest

/// Paths and scratch folders shared by the package tests. Everything runs
/// against the synthetic mock vault under `core/fixtures/`, copied into a
/// temporary folder whenever a test writes.
enum TestFixtures {
	/// `apps/apple/VaultdayCore/Tests/VaultdayCoreTests/<file>` → repo root.
	static let repoRoot = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
	static let mockVault = repoRoot.appending(path: "core/fixtures/mock-vault")

	/// A fresh temporary folder, removed when the test ends.
	static func temporaryFolder(_ test: XCTestCase, _ name: String = "scratch") throws -> URL {
		let folder = FileManager.default.temporaryDirectory
			.appending(path: "vaultday-tests-\(UUID().uuidString)", directoryHint: .isDirectory)
			.appending(path: name, directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
		test.addTeardownBlock {
			try? FileManager.default.removeItem(at: folder.deletingLastPathComponent())
		}
		return folder.resolvingSymlinksInPath()
	}

	/// A copy of the mock vault in a temporary folder.
	static func copyOfMockVault(_ test: XCTestCase) throws -> URL {
		let parent = try temporaryFolder(test, "vaults")
		let copy = parent.appending(path: "mock-vault", directoryHint: .isDirectory)
		try FileManager.default.copyItem(at: mockVault, to: copy)
		return copy
	}
}
