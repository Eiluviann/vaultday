import Foundation

/// The platform services the core runs on: the Keychain secret store, file
/// access inside granted folders, folder grants and the App Group layout.
/// Built once by the app (``forThisMac(bundle:)``) or by tests
/// (``inMemory(container:)``), then handed to ``CoreSession``.
public final class CoreEnvironment: Sendable {
	public let layout: AppGroupLayout
	public let grants: FolderGrants
	public let files: CoordinatedSyncFiles
	/// Whether at-rest copies of keys are sealed by the Secure Enclave.
	public let isHardwareWrapped: Bool
	/// Set when an unsigned Debug build fell back to an in-memory Keychain.
	public let unsignedFallbackNotice: String?
	let secrets: any SecretStorePort

	init(layout: AppGroupLayout, secrets: any SecretStorePort, isHardwareWrapped: Bool, unsignedFallbackNotice: String? = nil) {
		self.layout = layout
		self.unsignedFallbackNotice = unsignedFallbackNotice
		self.secrets = secrets
		self.isHardwareWrapped = isHardwareWrapped
		grants = FolderGrants()
		files = CoordinatedSyncFiles(roots: [layout.container])
	}

	/// The app's environment: the App Group container and Keychain access
	/// group named in Info.plist (`VaultdayAppGroup`, `VaultdayKeychainAccessGroup`).
	///
	/// - Parameter allowsUnsignedFallback: Debug builds pass `true`: a build
	///   signed without a development team has no App Group or Keychain
	///   entitlement, and then runs on the sandbox's own container and an
	///   in-memory Keychain, saying so in ``unsignedFallbackNotice``. Keys made
	///   in such a run are gone when it quits.
	public static func forThisMac(bundle: Bundle = .main, allowsUnsignedFallback: Bool = false) throws(CoreSetupError) -> CoreEnvironment {
		guard let group = bundle.object(forInfoDictionaryKey: "VaultdayAppGroup") as? String,
		      let accessGroup = bundle.object(forInfoDictionaryKey: "VaultdayKeychainAccessGroup") as? String,
		      !group.isEmpty, !accessGroup.isEmpty
		else { throw .missingConfiguration }
		let isEntitled = SecItemKeychainBackend.isEntitled(accessGroup: accessGroup)
		if !isEntitled, allowsUnsignedFallback {
			return try unsignedFallback()
		}
		guard let layout = AppGroupLayout.appGroup(group) else { throw .noAppGroupContainer(group) }
		do {
			try layout.createFolders()
		} catch {
			throw .containerNotWritable(layout.container.path)
		}
		let store: KeychainSecretStore
		do {
			store = try KeychainSecretStore.forThisMac(accessGroup: accessGroup)
		} catch {
			throw .keychainUnavailable(String(describing: error))
		}
		return CoreEnvironment(layout: layout, secrets: store, isHardwareWrapped: store.isHardwareWrapped)
	}

	private static func unsignedFallback() throws(CoreSetupError) -> CoreEnvironment {
		// A fresh folder per run: the keys of an earlier run are gone, so its
		// app state could never be opened again.
		let container = FileManager.default.temporaryDirectory
			.appending(path: "Vaultday unsigned run \(UUID().uuidString.prefix(8))", directoryHint: .isDirectory)
		do {
			return try inMemory(
				container: container,
				unsignedFallbackNotice: "This build is not signed with a development team: keys and app state last for this run only.",
			)
		} catch {
			throw .containerNotWritable(container.path)
		}
	}

	/// An environment over an in-memory Keychain and a plain folder, for
	/// tests and for CI, where the Keychain is not reachable. Keys at rest are
	/// wrapped by a software key kept in that Keychain.
	public static func inMemory(
		container: URL,
		keychain: InMemoryKeychainBackend = InMemoryKeychainBackend(),
		unsignedFallbackNotice: String? = nil,
	) throws -> CoreEnvironment {
		let layout = AppGroupLayout(container: container)
		try layout.createFolders()
		let accessGroup = "group.com.eiluviann.vaultday"
		let wrapper = try ECIESKeyWrapper.software(
			backend: keychain,
			address: KeychainItemAddress(
				service: "\(KeychainSecretStore.defaultServicePrefix).wrapping-key",
				account: "software-p256",
				accessGroup: accessGroup,
				isSynchronizable: false,
			),
		)
		let store = KeychainSecretStore(backend: keychain, accessGroup: accessGroup, wrapper: wrapper)
		return CoreEnvironment(layout: layout, secrets: store, isHardwareWrapped: store.isHardwareWrapped, unsignedFallbackNotice: unsignedFallbackNotice)
	}
}

/// Why the app could not set up its platform services.
public enum CoreSetupError: Error, Equatable, LocalizedError {
	case missingConfiguration
	case noAppGroupContainer(String)
	case containerNotWritable(String)
	case keychainUnavailable(String)

	public var errorDescription: String? {
		switch self {
		case .missingConfiguration:
			"This build of Vaultday names no App Group or Keychain access group."
		case let .noAppGroupContainer(group):
			"Vaultday is not entitled to its App Group container (\(group)); sign the app with a development team."
		case let .containerNotWritable(path):
			"Vaultday could not create its folders in \(path)."
		case let .keychainUnavailable(reason):
			"Vaultday could not reach the Keychain: \(reason)"
		}
	}
}
