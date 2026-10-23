import Foundation
import Security

/// Where one generic-password item lives. Every query names all four
/// attributes, so no query ever falls back to a default access group or
/// matches both synchronisable and device-only items.
public struct KeychainItemAddress: Hashable, Sendable, CustomStringConvertible {
	public let service: String
	public let account: String
	public let accessGroup: String
	public let isSynchronizable: Bool

	public init(service: String, account: String, accessGroup: String, isSynchronizable: Bool) {
		self.service = service
		self.account = account
		self.accessGroup = accessGroup
		self.isSynchronizable = isSynchronizable
	}

	public var description: String {
		"\(service) / \(account)\(isSynchronizable ? " (synchronizable)" : "")"
	}
}

/// When an item can be read. Synchronisable items cannot be
/// `ThisDeviceOnly`; device-only items always are.
public enum KeychainAccessibility: Sendable, Equatable {
	case afterFirstUnlock
	case afterFirstUnlockThisDeviceOnly
}

/// A Keychain call failed; carries the `OSStatus`, never item data.
public struct KeychainError: Error, Sendable, CustomStringConvertible {
	public let operation: String
	public let status: OSStatus

	public var description: String {
		let message = SecCopyErrorMessageString(status, nil).map { $0 as String } ?? "unknown error"
		return "\(operation) failed: \(message) (\(status))"
	}
}

/// The generic-password operations the secret store needs. The real
/// backend calls `SecItem*` (``SecItemKeychainBackend``); tests and CI
/// (where an unsigned test bundle has no Keychain entitlement) use
/// ``InMemoryKeychainBackend``.
public protocol KeychainBackend: Sendable {
	/// The item's data, `nil` when there is no such item.
	func data(at address: KeychainItemAddress) throws -> Data?
	/// Whether the item exists, without reading its data.
	func exists(_ address: KeychainItemAddress) throws -> Bool
	/// When the item was last written (here, or by iCloud Keychain bringing
	/// another device's version), `nil` when there is no such item. Read
	/// without the item's data.
	func modificationDate(at address: KeychainItemAddress) throws -> Date?
	/// Adds an item; `false` when one already exists at `address`.
	func add(_ data: Data, at address: KeychainItemAddress, accessibility: KeychainAccessibility) throws -> Bool
	/// Replaces an existing item's data; `false` when there is none.
	func update(_ data: Data, at address: KeychainItemAddress) throws -> Bool
	/// Deletes an item; a missing one is not an error.
	func delete(_ address: KeychainItemAddress) throws
	/// The accounts of every item of `service`, either synchronisable or not.
	func accounts(service: String, accessGroup: String, isSynchronizable: Bool) throws -> [String]
}

extension KeychainBackend {
	/// Adds the item, or replaces the data of the one already there.
	func upsert(_ data: Data, at address: KeychainItemAddress, accessibility: KeychainAccessibility) throws {
		if try !add(data, at: address, accessibility: accessibility) {
			_ = try update(data, at: address)
		}
	}
}

/// Which of the Mac's two keychain implementations the `SecItem` calls reach
/// (TN3137). The data protection keychain needs an App ID, so a provisioning
/// profile; the file-based login keychain works for ad-hoc signed and
/// unsigned builds, has no access groups and never synchronises.
public enum KeychainImplementation: Sendable, Equatable {
	case dataProtection
	case fileBased
}

/// ``KeychainBackend`` over `SecItem*`, on the data protection keychain or,
/// for a build signed without a provisioning profile, the file-based one.
public struct SecItemKeychainBackend: KeychainBackend {
	public let implementation: KeychainImplementation

	public init(implementation: KeychainImplementation = .dataProtection) {
		self.implementation = implementation
	}

	public func data(at address: KeychainItemAddress) throws -> Data? {
		var query = Self.query(for: address, in: implementation)
		query[kSecReturnData as String] = true
		query[kSecMatchLimit as String] = kSecMatchLimitOne
		var result: CFTypeRef?
		let status = SecItemCopyMatching(query as CFDictionary, &result)
		switch status {
		case errSecSuccess:
			return result as? Data
		case errSecItemNotFound:
			return nil
		default:
			throw KeychainError(operation: "SecItemCopyMatching", status: status)
		}
	}

	public func exists(_ address: KeychainItemAddress) throws -> Bool {
		var query = Self.query(for: address, in: implementation)
		query[kSecReturnAttributes as String] = true
		query[kSecMatchLimit as String] = kSecMatchLimitOne
		let status = SecItemCopyMatching(query as CFDictionary, nil)
		switch status {
		case errSecSuccess:
			return true
		case errSecItemNotFound:
			return false
		default:
			throw KeychainError(operation: "SecItemCopyMatching", status: status)
		}
	}

	public func modificationDate(at address: KeychainItemAddress) throws -> Date? {
		var query = Self.query(for: address, in: implementation)
		query[kSecReturnAttributes as String] = true
		query[kSecMatchLimit as String] = kSecMatchLimitOne
		var result: CFTypeRef?
		let status = SecItemCopyMatching(query as CFDictionary, &result)
		switch status {
		case errSecSuccess:
			let attributes = result as? [String: Any]
			return attributes?[kSecAttrModificationDate as String] as? Date ?? .distantPast
		case errSecItemNotFound:
			return nil
		default:
			throw KeychainError(operation: "SecItemCopyMatching", status: status)
		}
	}

	public func add(_ data: Data, at address: KeychainItemAddress, accessibility: KeychainAccessibility) throws -> Bool {
		var query = Self.query(for: address, in: implementation)
		query[kSecValueData as String] = data
		// The file-based keychain has no data protection classes.
		if implementation == .dataProtection {
			query[kSecAttrAccessible as String] = Self.accessibleValue(accessibility)
		}
		let status = SecItemAdd(query as CFDictionary, nil)
		switch status {
		case errSecSuccess:
			return true
		case errSecDuplicateItem:
			return false
		default:
			throw KeychainError(operation: "SecItemAdd", status: status)
		}
	}

	public func update(_ data: Data, at address: KeychainItemAddress) throws -> Bool {
		let attributes = [kSecValueData as String: data]
		let status = SecItemUpdate(Self.query(for: address, in: implementation) as CFDictionary, attributes as CFDictionary)
		switch status {
		case errSecSuccess:
			return true
		case errSecItemNotFound:
			return false
		default:
			throw KeychainError(operation: "SecItemUpdate", status: status)
		}
	}

	public func delete(_ address: KeychainItemAddress) throws {
		let status = SecItemDelete(Self.query(for: address, in: implementation) as CFDictionary)
		guard status == errSecSuccess || status == errSecItemNotFound else {
			throw KeychainError(operation: "SecItemDelete", status: status)
		}
	}

	public func accounts(service: String, accessGroup: String, isSynchronizable: Bool) throws -> [String] {
		var query = Self.serviceQuery(service: service, accessGroup: accessGroup, isSynchronizable: isSynchronizable, in: implementation)
		query[kSecReturnAttributes as String] = true
		query[kSecMatchLimit as String] = kSecMatchLimitAll
		var result: CFTypeRef?
		let status = SecItemCopyMatching(query as CFDictionary, &result)
		switch status {
		case errSecSuccess:
			let items = result as? [[String: Any]] ?? []
			return items.compactMap { $0[kSecAttrAccount as String] as? String }.sorted()
		case errSecItemNotFound:
			return []
		default:
			throw KeychainError(operation: "SecItemCopyMatching", status: status)
		}
	}

	/// Whether this process may write into `accessGroup` on the data
	/// protection keychain: `false` for a build without a provisioning
	/// profile that grants it. A write, not a read: an unentitled read answers
	/// `errSecItemNotFound` as if it were allowed. Any other failure answers
	/// `true` and surfaces, with its status, at the first real write.
	public static func isEntitled(accessGroup: String) -> Bool {
		let probe = KeychainItemAddress(service: "com.eiluviann.vaultday.entitlement-probe", account: "probe", accessGroup: accessGroup, isSynchronizable: false)
		var item = query(for: probe)
		item[kSecValueData as String] = Data()
		item[kSecAttrAccessible as String] = accessibleValue(.afterFirstUnlockThisDeviceOnly)
		let status = SecItemAdd(item as CFDictionary, nil)
		if status == errSecSuccess {
			SecItemDelete(query(for: probe) as CFDictionary)
		}
		return status != errSecMissingEntitlement
	}

	/// What a lookup on the file-based login keychain answers this process.
	public static func fileBasedKeychainStatus() -> OSStatus {
		let probe = KeychainItemAddress(service: "com.eiluviann.vaultday.entitlement-probe", account: "probe", accessGroup: "", isSynchronizable: false)
		var query = query(for: probe, in: .fileBased)
		query[kSecMatchLimit as String] = kSecMatchLimitOne
		return SecItemCopyMatching(query as CFDictionary, nil)
	}

	/// Whether `status` says there is no keychain at all (a CI runner without
	/// a login keychain), as opposed to one that is locked or refused, which
	/// may answer differently at the next launch.
	public static func meansNoKeychain(_ status: OSStatus) -> Bool {
		status == errSecNoSuchKeychain || status == errSecNotAvailable
	}

	/// The query naming exactly one item: class, service, account, access
	/// group, synchronizable and the data protection keychain, all explicit.
	/// On the file-based keychain only class, service and account exist.
	static func query(for address: KeychainItemAddress, in implementation: KeychainImplementation = .dataProtection) -> [String: Any] {
		var query = serviceQuery(service: address.service, accessGroup: address.accessGroup, isSynchronizable: address.isSynchronizable, in: implementation)
		query[kSecAttrAccount as String] = address.account
		return query
	}

	static func serviceQuery(service: String, accessGroup: String, isSynchronizable: Bool, in implementation: KeychainImplementation = .dataProtection) -> [String: Any] {
		switch implementation {
		case .dataProtection:
			return [
				kSecClass as String: kSecClassGenericPassword,
				kSecAttrService as String: service,
				kSecAttrAccessGroup as String: accessGroup,
				kSecAttrSynchronizable as String: isSynchronizable ? kCFBooleanTrue as Any : kCFBooleanFalse as Any,
				kSecUseDataProtectionKeychain as String: true,
			]
		case .fileBased:
			precondition(!isSynchronizable, "the file-based keychain never synchronises; the store must ask for device-only items")
			return [
				kSecClass as String: kSecClassGenericPassword,
				kSecAttrService as String: service,
				kSecUseDataProtectionKeychain as String: false,
			]
		}
	}

	static func accessibleValue(_ accessibility: KeychainAccessibility) -> CFString {
		switch accessibility {
		case .afterFirstUnlock: kSecAttrAccessibleAfterFirstUnlock
		case .afterFirstUnlockThisDeviceOnly: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
		}
	}
}

/// A ``KeychainBackend`` in memory that models iCloud Keychain: backends made
/// with ``forAnotherDevice()`` share every synchronizable item and keep their
/// own device-only items.
public final class InMemoryKeychainBackend: KeychainBackend, @unchecked Sendable {
	public struct StoredItem: Equatable, Sendable {
		public let data: Data
		public let accessibility: KeychainAccessibility
		public let modifiedAt: Date
	}

	private final class Partition: @unchecked Sendable {
		var items: [KeychainItemAddress: StoredItem] = [:]
		/// A logical clock, so two writes in one instant still differ.
		var revision = 0

		func stamp() -> Date {
			revision += 1
			return Date(timeIntervalSinceReferenceDate: TimeInterval(revision))
		}
	}

	// One lock for both partitions, shared by every device of the same
	// synchronised set, so a cross-device add is as atomic as SecItemAdd.
	private let lock: NSLock
	private let synchronized: Partition
	private let deviceOnly = Partition()

	public init() {
		lock = NSLock()
		synchronized = Partition()
	}

	private init(sharing other: InMemoryKeychainBackend) {
		lock = other.lock
		synchronized = other.synchronized
	}

	/// A backend for a second device of the same user.
	public func forAnotherDevice() -> InMemoryKeychainBackend {
		InMemoryKeychainBackend(sharing: self)
	}

	/// Every item this device sees, for assertions in tests.
	public var storedItems: [KeychainItemAddress: StoredItem] {
		withLock { synchronized.items.merging(deviceOnly.items) { first, _ in first } }
	}

	public func data(at address: KeychainItemAddress) throws -> Data? {
		withLock { partition(for: address).items[address]?.data }
	}

	public func exists(_ address: KeychainItemAddress) throws -> Bool {
		withLock { partition(for: address).items[address] != nil }
	}

	public func modificationDate(at address: KeychainItemAddress) throws -> Date? {
		withLock { partition(for: address).items[address]?.modifiedAt }
	}

	/// Replaces a synchronizable item as iCloud Keychain does when another
	/// device's version wins a conflict, for tests.
	public func replaceFromAnotherDevice(_ data: Data, at address: KeychainItemAddress) {
		withLock {
			let partition = partition(for: address)
			let accessibility = partition.items[address]?.accessibility ?? .afterFirstUnlock
			partition.items[address] = StoredItem(data: data, accessibility: accessibility, modifiedAt: partition.stamp())
		}
	}

	public func add(_ data: Data, at address: KeychainItemAddress, accessibility: KeychainAccessibility) throws -> Bool {
		// SecItemAdd refuses a synchronizable item that is ThisDeviceOnly.
		if address.isSynchronizable, accessibility == .afterFirstUnlockThisDeviceOnly {
			throw KeychainError(operation: "SecItemAdd", status: errSecParam)
		}
		return withLock {
			let partition = partition(for: address)
			guard partition.items[address] == nil else { return false }
			partition.items[address] = StoredItem(data: data, accessibility: accessibility, modifiedAt: partition.stamp())
			return true
		}
	}

	public func update(_ data: Data, at address: KeychainItemAddress) throws -> Bool {
		withLock {
			let partition = partition(for: address)
			guard let existing = partition.items[address] else { return false }
			partition.items[address] = StoredItem(data: data, accessibility: existing.accessibility, modifiedAt: partition.stamp())
			return true
		}
	}

	public func delete(_ address: KeychainItemAddress) throws {
		_ = withLock { partition(for: address).items.removeValue(forKey: address) }
	}

	public func accounts(service: String, accessGroup: String, isSynchronizable: Bool) throws -> [String] {
		withLock {
			(isSynchronizable ? synchronized : deviceOnly).items.keys
				.filter { $0.service == service && $0.accessGroup == accessGroup && $0.isSynchronizable == isSynchronizable }
				.map(\.account)
				.sorted()
		}
	}

	private func partition(for address: KeychainItemAddress) -> Partition {
		address.isSynchronizable ? synchronized : deviceOnly
	}

	private func withLock<T>(_ body: () -> T) -> T {
		lock.lock()
		defer { lock.unlock() }
		return body()
	}
}
