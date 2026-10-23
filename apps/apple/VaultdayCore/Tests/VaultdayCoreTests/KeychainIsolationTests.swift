import Foundation
import XCTest

/// No test reaches the real keychain: every test source (the package tests,
/// the app tests and the window tests) keeps to ``InMemoryKeychainBackend``
/// and plain bookmarks. A call that would reach the login or data protection
/// keychain prompts on a developer's Mac once per item and signature, and
/// would read or write their real items (docs/building.md, "Quiet tests").
final class KeychainIsolationTests: XCTestCase {
	/// `apps/apple/VaultdayCore/Tests/VaultdayCoreTests/<file>` → `apps/apple`.
	private static let appleRoot = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
	private static let testFolders = [
		"VaultdayCore/Tests/VaultdayCoreTests",
		"VaultdayTests",
		"VaultdayWindowTests",
	]

	/// Calls that reach a real keychain: the `SecItem` backend and its probes,
	/// the environments and stores built for this Mac, `SecItem*` and
	/// `SecKeychain*` themselves, and security-scoped bookmarks
	/// (ScopedBookmarkAgent keeps their keys in the login keychain).
	private static let keychainCalls = [
		#"\bSecItemKeychainBackend\s*\("#,
		#"\bSecItemKeychainBackend\.(isEntitled|fileBasedKeychainStatus)\s*\("#,
		#"\bSecItem(Add|CopyMatching|Update|Delete)\s*\("#,
		#"\bSecKeychain\w*\s*\("#,
		#"\.forThisMac\s*\("#,
		#"\bFolderGrants\s*\(\s*\)"#,
		#"usesSecurityScope:\s*true"#,
		#"\.withSecurityScope\b"#,
	]

	func testNoTestSourceReachesTheRealKeychain() throws {
		let files = try Self.testSources()
		XCTAssertGreaterThan(files.count, 20, "the test folders were not found under \(Self.appleRoot.path)")
		for file in files where file.lastPathComponent != "KeychainIsolationTests.swift" {
			let source = try String(contentsOf: file, encoding: .utf8)
			XCTAssertEqual(try Self.keychainCalls(in: source), [], "\(file.lastPathComponent) reaches the real keychain; use InMemoryKeychainBackend or FolderGrants(usesSecurityScope: false)")
		}
	}

	func testTheCheckCatchesARealKeychain() throws {
		XCTAssertEqual(try Self.keychainCalls(in: "let backend = SecItemKeychainBackend(implementation: .fileBased)").count, 1)
		XCTAssertEqual(try Self.keychainCalls(in: "let environment = try CoreEnvironment.forThisMac()").count, 1)
		XCTAssertEqual(try Self.keychainCalls(in: "SecItemAdd(query as CFDictionary, nil)").count, 1)
		XCTAssertEqual(try Self.keychainCalls(in: "let grants = FolderGrants()").count, 1)
		XCTAssertEqual(try Self.keychainCalls(in: "let backend = InMemoryKeychainBackend()"), [])
		XCTAssertEqual(try Self.keychainCalls(in: "let query = SecItemKeychainBackend.query(for: address)"), [], "building a query touches no keychain")
		XCTAssertEqual(try Self.keychainCalls(in: "FolderGrants(usesSecurityScope: Self.usesSecurityScope)"), [])
	}

	private static func testSources() throws -> [URL] {
		try testFolders.flatMap { folder in
			let url = appleRoot.appending(path: folder, directoryHint: .isDirectory)
			let enumerator = try XCTUnwrap(FileManager.default.enumerator(at: url, includingPropertiesForKeys: nil))
			return enumerator.compactMap { $0 as? URL }.filter { $0.pathExtension == "swift" }
		}
	}

	/// The patterns of ``keychainCalls`` that match `source`.
	private static func keychainCalls(in source: String) throws -> [String] {
		let range = NSRange(source.startIndex..., in: source)
		return try keychainCalls.filter { pattern in
			try NSRegularExpression(pattern: pattern).firstMatch(in: source, range: range) != nil
		}
	}
}
