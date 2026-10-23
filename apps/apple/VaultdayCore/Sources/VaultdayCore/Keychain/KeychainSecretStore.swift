import CryptoKit
import Foundation

/// The core's `SecretStore` over the Keychain.
///
/// Item layout, all generic passwords in one access group:
/// - A workspace or app data key: a synchronizable item
///   `<prefix>.<kind>` / `<name>` (how the user's other Apple devices get
///   it), plus a device-only copy `<prefix>.<kind>.wrapped` / `<name>`
///   sealed by this Mac's Secure Enclave key. Reads take the wrapped copy and
///   fall back to the synchronised item (re-wrapping it) when the copy is
///   missing or no longer opens; a key whose synchronised item is gone (the
///   workspace was removed on any device) reads as missing here too.
/// - A device public key: a synchronizable item, not wrapped (it is public).
/// - The device signing key seed: a device-only item, sealed by the
///   Secure Enclave key, never synchronised. One that no longer opens on this
///   Mac (a reset enclave) is deleted and reads as missing, so the core mints
///   a new device identity rather than failing at every launch.
///
/// Which kinds synchronise is the core's decision (`secretKindIsSynchronisable`);
/// a store made with `isSynchronising: false` (a build without the App
/// Group, see `docs/signing.md`) keeps the same layout with every item
/// device-only, so keys never leave this Mac.
/// Nothing here logs; errors carry `OSStatus` values, never key material.
public final class KeychainSecretStore: SecretStorePort {
	/// The service prefix of every item.
	public static let defaultServicePrefix = "com.eiluviann.vaultday"

	private let backend: any KeychainBackend
	private let accessGroup: String
	private let servicePrefix: String
	private let wrapper: (any KeyWrapping)?
	/// Whether the items of synchronisable kinds go to iCloud Keychain.
	public let isSynchronising: Bool

	/// - Parameters:
	///   - backend: `SecItemKeychainBackend` in the app, an in-memory one in tests.
	///   - accessGroup: the Keychain access group, from the app's Info.plist.
	///   - wrapper: the Secure Enclave wrapper; `nil` on a Mac without an
	///     enclave, where device-only items hold the raw bytes.
	///   - isSynchronising: `false` stores the items of synchronisable kinds
	///     device-only, for a keychain that cannot synchronise them.
	public init(
		backend: any KeychainBackend,
		accessGroup: String,
		servicePrefix: String = KeychainSecretStore.defaultServicePrefix,
		wrapper: (any KeyWrapping)?,
		isSynchronising: Bool = true,
	) {
		self.backend = backend
		self.accessGroup = accessGroup
		self.servicePrefix = servicePrefix
		self.wrapper = wrapper
		self.isSynchronising = isSynchronising
	}

	/// The app's store: the data protection keychain (or, signed without a
	/// profile, the file-based one), wrapped by the Secure Enclave when this
	/// Mac has one.
	public static func forThisMac(
		accessGroup: String,
		implementation: KeychainImplementation = .dataProtection,
		isSynchronising: Bool = true,
	) throws -> KeychainSecretStore {
		precondition(implementation == .dataProtection || !isSynchronising, "the file-based keychain cannot synchronise")
		let backend = SecItemKeychainBackend(implementation: implementation)
		let wrapper: (any KeyWrapping)? = try SecureEnclave.isAvailable
			? ECIESKeyWrapper.secureEnclave(
				backend: backend,
				address: KeychainItemAddress(
					service: "\(defaultServicePrefix).wrapping-key",
					account: "secure-enclave-p256",
					accessGroup: accessGroup,
					isSynchronizable: false,
				),
			)
			: nil
		return KeychainSecretStore(backend: backend, accessGroup: accessGroup, wrapper: wrapper, isSynchronising: isSynchronising)
	}

	/// Whether at-rest copies are sealed by the Secure Enclave.
	public var isHardwareWrapped: Bool { wrapper?.isHardwareBacked ?? false }

	// MARK: SecretStorePort

	public func get(id: SecretId) throws -> SecretItem? {
		try mapFailure {
			switch layout(of: id.kind) {
			case .synchronised:
				return try backend.data(at: syncedAddress(id)).map { item(id, $0) }
			case .synchronisedWithWrappedCopy:
				return try readWrappedCopy(id)
			case .deviceOnly:
				return try readDeviceOnly(id).map { item(id, $0) }
			}
		}
	}

	public func put(item: SecretItem) throws {
		try mapFailure {
			switch layout(of: item.id.kind) {
			case .synchronised:
				try backend.upsert(item.value, at: syncedAddress(item.id), accessibility: syncedAccessibility)
			case .synchronisedWithWrappedCopy:
				try backend.upsert(item.value, at: syncedAddress(item.id), accessibility: syncedAccessibility)
				try storeWrappedCopy(item)
			case .deviceOnly:
				let address = localAddress(item.id)
				try backend.upsert(try wrapped(item.value, at: address), at: address, accessibility: .afterFirstUnlockThisDeviceOnly)
			}
		}
	}

	public func add(item: SecretItem) throws -> Bool {
		try mapFailure {
			switch layout(of: item.id.kind) {
			case .synchronised:
				return try backend.add(item.value, at: syncedAddress(item.id), accessibility: syncedAccessibility)
			case .synchronisedWithWrappedCopy:
				guard try backend.add(item.value, at: syncedAddress(item.id), accessibility: syncedAccessibility) else {
					return false
				}
				try storeWrappedCopy(item)
				return true
			case .deviceOnly:
				let address = localAddress(item.id)
				return try backend.add(try wrapped(item.value, at: address), at: address, accessibility: .afterFirstUnlockThisDeviceOnly)
			}
		}
	}

	public func delete(id: SecretId) throws {
		try mapFailure {
			switch layout(of: id.kind) {
			case .synchronised:
				try backend.delete(syncedAddress(id))
			case .synchronisedWithWrappedCopy:
				try backend.delete(syncedAddress(id))
				try backend.delete(wrappedCopyAddress(id))
			case .deviceOnly:
				try backend.delete(localAddress(id))
			}
		}
	}

	public func list(kind: SecretKind) throws -> [SecretId] {
		try mapFailure {
			let isDeviceOnlyKind = layout(of: kind) == .deviceOnly
			let ids = try backend.accounts(service: service(of: kind), accessGroup: accessGroup, isSynchronizable: isSynchronising && !isDeviceOnlyKind)
				.sorted()
				.map { SecretId(kind: kind, name: $0) }
			// Decided by the kind, not by `isSynchronising`: a non-synchronising
			// store keeps a data key's raw item device-only too, and that one
			// is not sealed.
			guard isDeviceOnlyKind else { return ids }
			// Device-only items that no longer open are dropped, not listed.
			return try ids.filter { try readDeviceOnly($0) != nil }
		}
	}

	// MARK: Layout

	private enum Layout: Equatable {
		case synchronised
		case synchronisedWithWrappedCopy
		case deviceOnly
	}

	private func layout(of kind: SecretKind) -> Layout {
		guard secretKindIsSynchronisable(kind: kind) else { return .deviceOnly }
		// A public key protects nothing at rest; only secret material gets a wrapped copy.
		return kind == .devicePublicKey ? .synchronised : .synchronisedWithWrappedCopy
	}

	private func service(of kind: SecretKind) -> String {
		"\(servicePrefix).\(secretKindLabel(kind: kind))"
	}

	/// A synchronisable kind's item: readable after the first unlock, and
	/// bound to this Mac when the store does not synchronise.
	private var syncedAccessibility: KeychainAccessibility {
		isSynchronising ? .afterFirstUnlock : .afterFirstUnlockThisDeviceOnly
	}

	private func syncedAddress(_ id: SecretId) -> KeychainItemAddress {
		KeychainItemAddress(service: service(of: id.kind), account: id.name, accessGroup: accessGroup, isSynchronizable: isSynchronising)
	}

	private func wrappedCopyAddress(_ id: SecretId) -> KeychainItemAddress {
		KeychainItemAddress(service: "\(service(of: id.kind)).wrapped", account: id.name, accessGroup: accessGroup, isSynchronizable: false)
	}

	private func localAddress(_ id: SecretId) -> KeychainItemAddress {
		KeychainItemAddress(service: service(of: id.kind), account: id.name, accessGroup: accessGroup, isSynchronizable: false)
	}

	// MARK: Wrapped copies

	private func readWrappedCopy(_ id: SecretId) throws -> SecretItem? {
		let copyAddress = wrappedCopyAddress(id)
		guard let syncedDate = try backend.modificationDate(at: syncedAddress(id)) else {
			try backend.delete(copyAddress)
			return nil
		}
		if let wrapper, let stored = try backend.data(at: copyAddress),
		   let (stamp, blob) = Self.splitStamp(stored), stamp == syncedDate,
		   let value = try? wrapper.unwrap(blob, context: context(copyAddress)) {
			return item(id, value)
		}
		guard let value = try backend.data(at: syncedAddress(id)) else { return nil }
		let found = item(id, value)
		try storeWrappedCopy(found)
		return found
	}

	/// Writes the sealed copy of a data key, stamped with its synchronised item's date.
	private func storeWrappedCopy(_ item: SecretItem) throws {
		guard wrapper != nil, let syncedDate = try backend.modificationDate(at: syncedAddress(item.id)) else { return }
		let address = wrappedCopyAddress(item.id)
		let stored = Self.stamped(syncedDate, try wrapped(item.value, at: address))
		try backend.upsert(stored, at: address, accessibility: .afterFirstUnlockThisDeviceOnly)
	}

	/// A device-only item's bytes; one that does not open is deleted and reads as missing.
	private func readDeviceOnly(_ id: SecretId) throws -> Data? {
		let address = localAddress(id)
		guard let stored = try backend.data(at: address) else { return nil }
		guard let wrapper else { return stored }
		guard let value = try? wrapper.unwrap(stored, context: context(address)) else {
			try backend.delete(address)
			return nil
		}
		return value
	}

	private static func stamped(_ date: Date, _ blob: Data) -> Data {
		withUnsafeBytes(of: date.timeIntervalSinceReferenceDate.bitPattern.bigEndian) { Data($0) } + blob
	}

	private static func splitStamp(_ stored: Data) -> (Date, Data)? {
		let bytes = [UInt8](stored)
		guard bytes.count > 8 else { return nil }
		let bits = bytes.prefix(8).reduce(UInt64(0)) { $0 << 8 | UInt64($1) }
		return (Date(timeIntervalSinceReferenceDate: Double(bitPattern: bits)), Data(bytes.dropFirst(8)))
	}

	private func wrapped(_ value: Data, at address: KeychainItemAddress) throws -> Data {
		guard let wrapper else { return value }
		return try wrapper.wrap(value, context: context(address))
	}

	private func context(_ address: KeychainItemAddress) -> Data {
		Data("\(address.accessGroup)\u{1f}\(address.service)\u{1f}\(address.account)".utf8)
	}

	private func item(_ id: SecretId, _ value: Data) -> SecretItem {
		SecretItem(id: id, isSynchronisable: secretKindIsSynchronisable(kind: id.kind), value: value)
	}

	private func mapFailure<T>(_ body: () throws -> T) throws -> T {
		do {
			return try body()
		} catch let error as KeychainError {
			throw SecretStoreFailure.Platform(message: error.description)
		} catch let error as KeyWrappingError {
			throw SecretStoreFailure.Platform(message: "a wrapped key does not open on this Mac (\(error))")
		} catch let error as SecretStoreFailure {
			throw error
		} catch {
			throw SecretStoreFailure.Platform(message: String(describing: type(of: error)))
		}
	}
}
