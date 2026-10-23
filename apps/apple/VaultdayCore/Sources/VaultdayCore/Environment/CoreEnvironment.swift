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
	/// Where keys and app state live, from what the build is signed for.
	public let signingMode: SigningMode
	/// Set when the build runs in a degraded ``signingMode``: what it lacks and where its state is.
	public let signingNotice: String?
	let secrets: any SecretStorePort

	init(layout: AppGroupLayout, secrets: any SecretStorePort, isHardwareWrapped: Bool, grants: FolderGrants = FolderGrants(), signingMode: SigningMode = .inMemory) {
		self.layout = layout
		self.signingMode = signingMode
		signingNotice = signingMode.notice(stateFolder: layout.container)
		self.secrets = secrets
		self.isHardwareWrapped = isHardwareWrapped
		self.grants = grants
		files = CoordinatedSyncFiles(roots: [layout.container])
	}

	/// The app's environment, in the ``SigningMode`` the build's entitlements
	/// allow: the App Group container and Keychain access group named in
	/// Info.plist (`VaultdayAppGroup`, `VaultdayKeychainAccessGroup`) when
	/// the build claims the App Group; otherwise the team's own Keychain
	/// access group or the login keychain, with every key device-only and the
	/// state under `~/Library/Application Support/Vaultday`.
	///
	/// - Parameter allowsUnsignedFallback: Debug builds pass `true`: when
	///   there is no login keychain at all (not merely a locked one), the app runs on an in-memory
	///   Keychain and a fresh temporary folder, saying so in
	///   ``signingNotice``. Keys made in such a run are gone when it quits.
	public static func forThisMac(bundle: Bundle = .main, allowsUnsignedFallback: Bool = false) throws(CoreSetupError) -> CoreEnvironment {
		guard let group = bundle.object(forInfoDictionaryKey: "VaultdayAppGroup") as? String,
		      let accessGroup = bundle.object(forInfoDictionaryKey: "VaultdayKeychainAccessGroup") as? String,
		      !group.isEmpty, !accessGroup.isEmpty
		else { throw .missingConfiguration }
		let mode = SigningMode.resolve(entitlements: .ofThisProcess(), appGroup: group, preferredKeychainGroup: accessGroup)
		switch mode {
		case .appGroup:
			guard let layout = AppGroupLayout.appGroup(group) else { throw .noAppGroupContainer(group) }
			return try keychainEnvironment(layout: layout, accessGroup: accessGroup, implementation: .dataProtection, mode: mode)
		case let .teamKeychain(teamAccessGroup):
			return try keychainEnvironment(layout: localLayout(for: mode), accessGroup: teamAccessGroup, implementation: .dataProtection, mode: mode)
		case .loginKeychain, .inMemory:
			let status = SecItemKeychainBackend.fileBasedKeychainStatus()
			if SecItemKeychainBackend.meansNoKeychain(status) {
				guard allowsUnsignedFallback else { throw .keychainUnavailable("there is no login keychain") }
				return try unsignedFallback()
			}
			// A locked or refused keychain is not "no keychain": falling back
			// would hide this build's state behind a throwaway run.
			guard status == errSecSuccess || status == errSecItemNotFound else {
				throw .keychainUnavailable(KeychainError(operation: "SecItemCopyMatching", status: status).description)
			}
			return try keychainEnvironment(layout: localLayout(for: .loginKeychain), accessGroup: SigningMode.loginKeychainScope, implementation: .fileBased, mode: .loginKeychain)
		}
	}

	private static func keychainEnvironment(
		layout: AppGroupLayout,
		accessGroup: String,
		implementation: KeychainImplementation,
		mode: SigningMode,
	) throws(CoreSetupError) -> CoreEnvironment {
		if implementation == .dataProtection, !SecItemKeychainBackend.isEntitled(accessGroup: accessGroup) {
			throw .keychainGroupNotGranted(accessGroup)
		}
		do {
			try layout.createFolders()
		} catch {
			throw .containerNotWritable(layout.container.path)
		}
		let store: KeychainSecretStore
		do {
			store = try KeychainSecretStore.forThisMac(accessGroup: accessGroup, implementation: implementation, isSynchronising: mode.isSynchronising)
		} catch {
			throw .keychainUnavailable(String(describing: error))
		}
		return CoreEnvironment(layout: layout, secrets: store, isHardwareWrapped: store.isHardwareWrapped, signingMode: mode)
	}

	private static func localLayout(for mode: SigningMode) throws(CoreSetupError) -> AppGroupLayout {
		let candidates = AppGroupLayout.localStateRootCandidates()
		guard let folderName = mode.localStateFolderName, let root = AppGroupLayout.firstWritableRoot(candidates) else {
			throw .containerNotWritable(candidates.map(\.path).joined(separator: " or "))
		}
		return AppGroupLayout.local(root: root, folderName: folderName)
	}

	private static func unsignedFallback() throws(CoreSetupError) -> CoreEnvironment {
		// A fresh folder per run: the keys of an earlier run are gone, so its
		// app state could never be opened again.
		let container = FileManager.default.temporaryDirectory
			.appending(path: "Vaultday unsigned run \(UUID().uuidString.prefix(8))", directoryHint: .isDirectory)
		do {
			return try inMemory(container: container, grants: FolderGrants(), signingMode: .inMemory)
		} catch {
			throw .containerNotWritable(container.path)
		}
	}

	/// An environment over an in-memory Keychain and a plain folder, for
	/// tests and for CI, where the Keychain is not reachable. Keys at rest are
	/// wrapped by a software key kept in that Keychain. Folder grants are plain
	/// bookmarks unless `grants` says otherwise, for the same reason.
	public static func inMemory(
		container: URL,
		keychain: InMemoryKeychainBackend = InMemoryKeychainBackend(),
		grants: FolderGrants = FolderGrants(usesSecurityScope: false),
		signingMode: SigningMode = .appGroup("group.com.eiluviann.vaultday"),
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
		return CoreEnvironment(layout: layout, secrets: store, isHardwareWrapped: store.isHardwareWrapped, grants: grants, signingMode: signingMode)
	}
}

/// Why the app could not set up its platform services.
public enum CoreSetupError: Error, Equatable, LocalizedError {
	case missingConfiguration
	case noAppGroupContainer(String)
	case containerNotWritable(String)
	case keychainUnavailable(String)
	case keychainGroupNotGranted(String)

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
		case let .keychainGroupNotGranted(group):
			"Vaultday claims the Keychain access group \(group), but its provisioning profile does not grant it; see docs/signing.md."
		}
	}
}
