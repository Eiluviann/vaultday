import Foundation
import XCTest
@testable import VaultdayCore

/// A session over an in-memory environment that can be opened again, as a relaunch does.
@MainActor
struct OnboardingHarness {
	let container: URL
	let keychain: InMemoryKeychainBackend
	let session: CoreSession

	init(_ test: XCTestCase) throws {
		container = try TestFixtures.temporaryFolder(test, "Group")
		keychain = InMemoryKeychainBackend()
		session = try CoreSession.open(try CoreEnvironment.inMemory(container: container, keychain: keychain))
	}

	private init(container: URL, keychain: InMemoryKeychainBackend, session: CoreSession) {
		self.container = container
		self.keychain = keychain
		self.session = session
	}

	/// The same container and Keychain, opened again.
	func relaunched() throws -> OnboardingHarness {
		let session = try CoreSession.open(try CoreEnvironment.inMemory(container: container, keychain: keychain))
		return OnboardingHarness(container: container, keychain: keychain, session: session)
	}
}

extension TestFixtures {
	/// An empty folder with the given name inside a fresh temporary folder.
	static func emptyFolder(_ test: XCTestCase, named name: String) throws -> URL {
		let parent = try temporaryFolder(test, "folders")
		let folder = parent.appending(path: name, directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
		return folder
	}

	/// The keys of a workspace's `settings.json`, as parsed JSON.
	static func settingsJSON(in folder: URL) throws -> [String: Any] {
		let data = try Data(contentsOf: folder.appending(path: ".vaultday/settings.json"))
		return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
	}
}
