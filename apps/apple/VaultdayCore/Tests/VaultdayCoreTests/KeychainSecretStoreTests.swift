import CryptoKit
import Security
import XCTest
@testable import VaultdayCore

/// The Keychain store against ``InMemoryKeychainBackend``: an unsigned test
/// bundle has no Keychain entitlement (`SecItemAdd` answers -34018), on CI
/// and on a Mac without a development team alike. The `SecItem` queries
/// themselves are checked attribute by attribute.
final class KeychainSecretStoreTests: XCTestCase {
	private let group = "group.com.eiluviann.vaultday"

	private func store(_ backend: InMemoryKeychainBackend, wrapper: (any KeyWrapping)? = ECIESKeyWrapper.software()) -> KeychainSecretStore {
		KeychainSecretStore(backend: backend, accessGroup: group, wrapper: wrapper)
	}

	private func dataKey(_ name: String, byte: UInt8 = 7) -> SecretItem {
		SecretItem(id: SecretId(kind: .workspaceDataKey, name: name), isSynchronisable: true, value: Data(repeating: byte, count: 32))
	}

	func testADataKeySynchronisesAndKeepsAWrappedCopyOnThisMac() throws {
		let backend = InMemoryKeychainBackend()
		let keychain = store(backend)
		XCTAssertTrue(try keychain.add(item: dataKey("mock-studio")))

		let items = backend.storedItems
		let synced = try XCTUnwrap(items[KeychainItemAddress(service: "com.eiluviann.vaultday.workspace-data-key", account: "mock-studio", accessGroup: group, isSynchronizable: true)])
		XCTAssertEqual(synced.accessibility, .afterFirstUnlock)
		let copy = try XCTUnwrap(items[KeychainItemAddress(service: "com.eiluviann.vaultday.workspace-data-key.wrapped", account: "mock-studio", accessGroup: group, isSynchronizable: false)])
		XCTAssertEqual(copy.accessibility, .afterFirstUnlockThisDeviceOnly)
		XCTAssertNotEqual(copy.data, synced.data, "the copy at rest is sealed")
		XCTAssertFalse(copy.data.range(of: synced.data) != nil, "the sealed copy does not contain the key")

		XCTAssertEqual(try keychain.get(id: SecretId(kind: .workspaceDataKey, name: "mock-studio"))?.value, Data(repeating: 7, count: 32))
	}

	func testAddNeverOverwritesAndListIsSortedPerKind() throws {
		let keychain = store(InMemoryKeychainBackend())
		XCTAssertTrue(try keychain.add(item: dataKey("b")))
		XCTAssertTrue(try keychain.add(item: dataKey("a")))
		XCTAssertFalse(try keychain.add(item: dataKey("a", byte: 9)))
		XCTAssertEqual(try keychain.get(id: SecretId(kind: .workspaceDataKey, name: "a"))?.value.first, 7)
		XCTAssertEqual(try keychain.list(kind: .workspaceDataKey).map(\.name), ["a", "b"])
		XCTAssertEqual(try keychain.list(kind: .appDataKey), [])
	}

	func testTheSigningKeyStaysOnThisMacAndThePublicKeyIsPublished() throws {
		let backend = InMemoryKeychainBackend()
		let keychain = store(backend)
		let deviceId = try loadDeviceId(secrets: keychain)

		let signing = try XCTUnwrap(backend.storedItems.first { $0.key.service.hasSuffix(".device-signing-key") })
		XCTAssertFalse(signing.key.isSynchronizable)
		XCTAssertEqual(signing.key.account, deviceId)
		XCTAssertEqual(signing.value.accessibility, .afterFirstUnlockThisDeviceOnly)
		XCTAssertNotEqual(signing.value.data.count, 32, "the seed is stored sealed")

		// Published only once this Mac holds the app key, which opening the app state mints.
		XCTAssertNil(backend.storedItems.first { $0.key.service.hasSuffix(".device-public-key") })
		let appFolder = try TestFixtures.temporaryFolder(self)
		_ = try openAppState(files: CoordinatedSyncFiles(roots: [appFolder]), dir: appFolder.path, secrets: keychain)
		let published = try XCTUnwrap(backend.storedItems.first { $0.key.service.hasSuffix(".device-public-key") })
		XCTAssertTrue(published.key.isSynchronizable)
		XCTAssertEqual(published.value.data.count, 32, "a public key is not wrapped")

		let other = store(backend.forAnotherDevice())
		XCTAssertEqual(try other.list(kind: .deviceSigningKey), [], "device-only items do not travel")
		XCTAssertEqual(try other.list(kind: .devicePublicKey).map(\.name), [deviceId])
		XCTAssertEqual(try loadDeviceId(secrets: keychain), deviceId, "the identity is stable")
	}

	func testDeletingAWorkspaceOnAnotherMacRemovesItHereToo() throws {
		let backend = InMemoryKeychainBackend()
		let here = store(backend)
		let there = store(backend.forAnotherDevice())
		_ = try here.add(item: dataKey("mock-studio"))
		XCTAssertNotNil(try there.get(id: SecretId(kind: .workspaceDataKey, name: "mock-studio")))

		try deleteWorkspaceKeys(secrets: there, workspaceId: "mock-studio")

		XCTAssertNil(try here.get(id: SecretId(kind: .workspaceDataKey, name: "mock-studio")))
		XCTAssertFalse(backend.storedItems.keys.contains { $0.account == "mock-studio" }, "the wrapped copy goes with it")
	}

	func testAWrappedCopyThatNoLongerOpensIsRebuiltFromTheSynchronisedItem() throws {
		let backend = InMemoryKeychainBackend()
		_ = try store(backend, wrapper: ECIESKeyWrapper.software()).add(item: dataKey("mock-studio"))
		// A new wrapping key, as after the Mac's enclave was reset.
		let rewrapped = store(backend, wrapper: ECIESKeyWrapper.software())
		XCTAssertEqual(try rewrapped.get(id: SecretId(kind: .workspaceDataKey, name: "mock-studio"))?.value.first, 7)
		XCTAssertEqual(try rewrapped.get(id: SecretId(kind: .workspaceDataKey, name: "mock-studio"))?.value.first, 7)
	}

	func testAConflictResolvedOnAnotherDeviceReplacesTheWrappedCopy() throws {
		let backend = InMemoryKeychainBackend()
		let keychain = store(backend)
		let id = SecretId(kind: .workspaceDataKey, name: "mock-studio")
		_ = try keychain.add(item: dataKey("mock-studio", byte: 1))
		XCTAssertEqual(try keychain.get(id: id)?.value.first, 1)

		// iCloud Keychain resolves a conflict in favour of the other device's key.
		let synced = KeychainItemAddress(service: "com.eiluviann.vaultday.workspace-data-key", account: "mock-studio", accessGroup: group, isSynchronizable: true)
		backend.replaceFromAnotherDevice(Data(repeating: 2, count: 32), at: synced)

		XCTAssertEqual(try keychain.get(id: id)?.value.first, 2, "the sealed copy does not outlive the conflict")
		XCTAssertEqual(try keychain.get(id: id)?.value.first, 2)
	}

	func testASigningKeyThatNoLongerOpensYieldsANewDeviceIdentity() throws {
		let backend = InMemoryKeychainBackend()
		let first = try loadDeviceId(secrets: store(backend, wrapper: ECIESKeyWrapper.software()))
		// A reset enclave: the seed sealed by the old key no longer opens.
		let reset = store(backend, wrapper: ECIESKeyWrapper.software())
		XCTAssertEqual(try reset.list(kind: .deviceSigningKey), [])
		let second = try loadDeviceId(secrets: reset)
		XCTAssertNotEqual(first, second, "the core mints a new identity instead of failing")
		XCTAssertEqual(try reset.list(kind: .deviceSigningKey).map(\.name), [second])
	}

	func testAWrappedBlobIsBoundToItsItem() throws {
		let wrapper = ECIESKeyWrapper.software()
		let blob = try wrapper.wrap(Data([1, 2, 3]), context: Data("item-a".utf8))
		XCTAssertEqual(try wrapper.unwrap(blob, context: Data("item-a".utf8)), Data([1, 2, 3]))
		XCTAssertThrowsError(try wrapper.unwrap(blob, context: Data("item-b".utf8))) {
			XCTAssertEqual($0 as? KeyWrappingError, .notOpenable)
		}
		XCTAssertThrowsError(try wrapper.unwrap(Data([9]), context: Data())) {
			XCTAssertEqual($0 as? KeyWrappingError, .malformed)
		}
		XCTAssertThrowsError(try ECIESKeyWrapper.software().unwrap(blob, context: Data("item-a".utf8)))
	}

	func testTheSecureEnclaveWrapsWhereThisMacHasOne() throws {
		try XCTSkipUnless(SecureEnclave.isAvailable, "no Secure Enclave on this machine (CI runners)")
		let backend = InMemoryKeychainBackend()
		let address = KeychainItemAddress(service: "com.eiluviann.vaultday.wrapping-key", account: "secure-enclave-p256", accessGroup: group, isSynchronizable: false)
		let wrapper = try ECIESKeyWrapper.secureEnclave(backend: backend, address: address)
		XCTAssertTrue(wrapper.isHardwareBacked)
		let blob = try wrapper.wrap(Data(repeating: 5, count: 32), context: Data("ctx".utf8))
		let reloaded = try ECIESKeyWrapper.secureEnclave(backend: backend, address: address)
		XCTAssertEqual(try reloaded.unwrap(blob, context: Data("ctx".utf8)), Data(repeating: 5, count: 32))
		XCTAssertEqual(backend.storedItems[address]?.accessibility, .afterFirstUnlockThisDeviceOnly)
	}

	func testFailuresCarryTheStatusAndNoKeyMaterial() throws {
		let keychain = KeychainSecretStore(backend: FailingBackend(), accessGroup: group, wrapper: nil)
		XCTAssertThrowsError(try keychain.add(item: dataKey("mock-studio", byte: 0xAB))) { error in
			guard case let .Platform(message) = error as? SecretStoreFailure else { return XCTFail("\(error)") }
			XCTAssertTrue(message.contains("-34018"), message)
			XCTAssertFalse(message.contains("171") || message.lowercased().contains("abab"), message)
		}
	}

	func testEveryQueryNamesServiceAccountAccessGroupAndSynchronizable() {
		for isSynchronizable in [true, false] {
			let query = SecItemKeychainBackend.query(for: KeychainItemAddress(service: "s", account: "a", accessGroup: group, isSynchronizable: isSynchronizable))
			XCTAssertEqual(query[kSecClass as String] as? String, kSecClassGenericPassword as String)
			XCTAssertEqual(query[kSecAttrService as String] as? String, "s")
			XCTAssertEqual(query[kSecAttrAccount as String] as? String, "a")
			XCTAssertEqual(query[kSecAttrAccessGroup as String] as? String, group)
			XCTAssertEqual((query[kSecAttrSynchronizable as String] as? Bool), isSynchronizable)
			XCTAssertEqual(query[kSecUseDataProtectionKeychain as String] as? Bool, true)
		}
		XCTAssertEqual(SecItemKeychainBackend.accessibleValue(.afterFirstUnlockThisDeviceOnly), kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly)
	}

	func testANonSynchronisingStoreKeepsEveryItemOnThisMac() throws {
		let backend = InMemoryKeychainBackend()
		let keychain = KeychainSecretStore(backend: backend, accessGroup: group, wrapper: ECIESKeyWrapper.software(), isSynchronising: false)
		XCTAssertTrue(try keychain.add(item: dataKey("mock-studio")))
		_ = try loadDeviceId(secrets: keychain)
		try keychain.put(item: SecretItem(id: SecretId(kind: .devicePublicKey, name: "this-mac"), isSynchronisable: true, value: Data(repeating: 3, count: 32)))

		XCTAssertFalse(backend.storedItems.isEmpty)
		XCTAssertFalse(backend.storedItems.keys.contains(where: \.isSynchronizable), "nothing reaches iCloud Keychain")
		XCTAssertTrue(backend.storedItems.values.allSatisfy { $0.accessibility == .afterFirstUnlockThisDeviceOnly }, "every item is bound to this Mac")
		XCTAssertNil(try backend.forAnotherDevice().data(at: KeychainItemAddress(service: "com.eiluviann.vaultday.workspace-data-key", account: "mock-studio", accessGroup: group, isSynchronizable: true)))
		XCTAssertEqual(try keychain.get(id: SecretId(kind: .workspaceDataKey, name: "mock-studio"))?.value, Data(repeating: 7, count: 32))
		XCTAssertEqual(try keychain.list(kind: .workspaceDataKey).map(\.name), ["mock-studio"])
		XCTAssertEqual(try keychain.list(kind: .devicePublicKey).map(\.name), ["this-mac"])

		try keychain.delete(id: SecretId(kind: .workspaceDataKey, name: "mock-studio"))
		XCTAssertNil(try keychain.get(id: SecretId(kind: .workspaceDataKey, name: "mock-studio")))
		XCTAssertFalse(backend.storedItems.keys.contains { $0.account == "mock-studio" }, "the wrapped copy goes with it")
	}

	func testFileBasedQueriesNameOnlyClassServiceAndAccount() {
		let query = SecItemKeychainBackend.query(for: KeychainItemAddress(service: "s", account: "a", accessGroup: "login-keychain", isSynchronizable: false), in: .fileBased)
		XCTAssertEqual(query[kSecClass as String] as? String, kSecClassGenericPassword as String)
		XCTAssertEqual(query[kSecAttrService as String] as? String, "s")
		XCTAssertEqual(query[kSecAttrAccount as String] as? String, "a")
		XCTAssertEqual(query[kSecUseDataProtectionKeychain as String] as? Bool, false)
		XCTAssertNil(query[kSecAttrAccessGroup as String], "the file-based keychain has no access groups")
		XCTAssertNil(query[kSecAttrSynchronizable as String])
	}

	func testTheInMemoryBackendRefusesASynchronisableDeviceOnlyItemLikeSecItemAdd() {
		let address = KeychainItemAddress(service: "s", account: "a", accessGroup: group, isSynchronizable: true)
		XCTAssertThrowsError(try InMemoryKeychainBackend().add(Data(), at: address, accessibility: .afterFirstUnlockThisDeviceOnly))
	}
}

private struct FailingBackend: KeychainBackend {
	private var failure: KeychainError { KeychainError(operation: "SecItemAdd", status: errSecMissingEntitlement) }
	func data(at _: KeychainItemAddress) throws -> Data? { throw failure }
	func modificationDate(at _: KeychainItemAddress) throws -> Date? { throw failure }
	func exists(_: KeychainItemAddress) throws -> Bool { throw failure }
	func add(_: Data, at _: KeychainItemAddress, accessibility _: KeychainAccessibility) throws -> Bool { throw failure }
	func update(_: Data, at _: KeychainItemAddress) throws -> Bool { throw failure }
	func delete(_: KeychainItemAddress) throws { throw failure }
	func accounts(service _: String, accessGroup _: String, isSynchronizable _: Bool) throws -> [String] { throw failure }
}
