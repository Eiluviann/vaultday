import Foundation
import Security

/// The entitlements this process's signature claims, read back through
/// `SecTask`. A build signed without a provisioning profile claims fewer of
/// them (`apps/apple/Signing/*.entitlements`), and the app picks its storage
/// from what is claimed rather than from probing, so the choice is the same
/// at every launch of one build.
public struct SigningEntitlements: Sendable, Equatable {
	public let isSandboxed: Bool
	public let applicationGroups: [String]
	public let keychainAccessGroups: [String]

	public init(isSandboxed: Bool, applicationGroups: [String], keychainAccessGroups: [String]) {
		self.isSandboxed = isSandboxed
		self.applicationGroups = applicationGroups
		self.keychainAccessGroups = keychainAccessGroups
	}

	/// What the running app's signature claims; nothing for an unsigned build.
	public static func ofThisProcess() -> SigningEntitlements {
		guard let task = SecTaskCreateFromSelf(nil) else {
			return SigningEntitlements(isSandboxed: false, applicationGroups: [], keychainAccessGroups: [])
		}
		return SigningEntitlements(
			isSandboxed: value(of: "com.apple.security.app-sandbox", in: task) as? Bool ?? false,
			applicationGroups: value(of: "com.apple.security.application-groups", in: task) as? [String] ?? [],
			keychainAccessGroups: value(of: "keychain-access-groups", in: task) as? [String] ?? [],
		)
	}

	private static func value(of entitlement: String, in task: SecTask) -> Any? {
		SecTaskCopyValueForEntitlement(task, entitlement as CFString, nil)
	}
}

/// Where keys and app state live, decided by what the build is signed for
/// (`docs/signing.md`). Each mode degrades one entitlement at a time:
/// - ``appGroup(_:)``: the build claims the App Group its Info.plist names
///   (the shipped one for a paid team, a team-prefixed one for a Personal
///   Team). The App Group container, the Info.plist Keychain access group on
///   the data protection keychain, data keys synchronised through iCloud Keychain.
/// - ``teamKeychain(accessGroup:)``: a team build without the App Group
///   (`Signing/PersonalTeam.entitlements`). The data protection keychain in
///   the team's own access group, every item device-only, state in
///   `~/Library/Application Support/Vaultday`.
/// - ``loginKeychain``: no profile at all (ad-hoc signed, or unsigned). The
///   file-based login keychain, every item device-only, state as above.
/// - ``inMemory``: Debug only, when not even the login keychain answers.
public enum SigningMode: Sendable, Equatable {
	case appGroup(String)
	case teamKeychain(accessGroup: String)
	case loginKeychain
	case inMemory

	/// The access group the login-keychain store names in its item addresses
	/// (the file-based keychain has none; it only binds the wrapped blobs).
	public static let loginKeychainScope = "login-keychain"

	/// The mode a build claiming `entitlements` runs in. `appGroup` is the
	/// group named in Info.plist. Of the keychain groups, `preferredKeychainGroup`
	/// (Info.plist's) wins when claimed, so the choice does not hang on the
	/// entitlement's order; a wildcard group (`TEAM.*`) names no group the app
	/// can write to, so it is skipped.
	public static func resolve(entitlements: SigningEntitlements, appGroup: String, preferredKeychainGroup: String? = nil) -> SigningMode {
		if entitlements.applicationGroups.contains(appGroup) {
			return .appGroup(appGroup)
		}
		let concreteGroups = entitlements.keychainAccessGroups.filter { !$0.hasSuffix("*") && !$0.isEmpty }
		if let accessGroup = concreteGroups.first(where: { $0 == preferredKeychainGroup }) ?? concreteGroups.first {
			return .teamKeychain(accessGroup: accessGroup)
		}
		return .loginKeychain
	}

	/// Whether data keys go to iCloud Keychain (and so to the user's other devices).
	public var isSynchronising: Bool {
		if case .appGroup = self { return true }
		return false
	}

	/// The folder, under the local state root, that holds this mode's app
	/// state. Each keychain gets its own, since app state only opens with the
	/// keys of the keychain that wrote it; `nil` for the App Group container.
	public var localStateFolderName: String? {
		switch self {
		case .appGroup, .inMemory: nil
		case let .teamKeychain(accessGroup): accessGroup
		case .loginKeychain: Self.loginKeychainScope
		}
	}

	/// The line the sidebar shows for a degraded mode, `nil` for the full one.
	public func notice(stateFolder: URL) -> String? {
		switch self {
		case .appGroup:
			nil
		case .teamKeychain:
			"Signed without the App Group: keys stay on this Mac (no iCloud Keychain), app state is in \(stateFolder.path)."
		case .loginKeychain:
			"Signed without a provisioning profile: keys are in the login keychain of this Mac only, app state is in \(stateFolder.path)."
		case .inMemory:
			"No keychain is reachable: keys and app state last for this run only."
		}
	}
}

extension AppGroupLayout {
	/// The folder a build without the App Group keeps its state under:
	/// `~/Library/Application Support/Vaultday` of the real home (the
	/// `Signing/*.entitlements` variants grant it as a temporary exception),
	/// else the same path inside the sandbox container.
	public static func localStateRootCandidates(realHome: URL = realHomeFolder(), applicationSupport: URL? = sandboxApplicationSupportFolder()) -> [URL] {
		let inRealHome = realHome.appending(path: "Library/Application Support/Vaultday", directoryHint: .isDirectory)
		let inContainer = applicationSupport?.appending(path: "Vaultday", directoryHint: .isDirectory)
		return [inRealHome, inContainer].compactMap(\.self).reduce(into: []) { unique, url in
			if !unique.contains(url) { unique.append(url) }
		}
	}

	/// The first of `candidates` this process can create and write, `nil` when none.
	public static func firstWritableRoot(_ candidates: [URL]) -> URL? {
		candidates.first { candidate in
			do {
				try FileManager.default.createDirectory(at: candidate, withIntermediateDirectories: true)
				return FileManager.default.isWritableFile(atPath: candidate.path)
			} catch {
				return false
			}
		}
	}

	/// The layout under a local state root: app state in `<root>/<folderName>`,
	/// the API files in `<root>/API` whichever keychain is in use, so the CLI
	/// looks in one place (`~/Library/Application Support/Vaultday/API`).
	public static func local(root: URL, folderName: String) -> AppGroupLayout {
		AppGroupLayout(
			container: root.appending(path: folderName, directoryHint: .isDirectory),
			apiFolder: root.appending(path: "API", directoryHint: .isDirectory),
		)
	}

	/// The user's home folder, not the sandbox container `NSHomeDirectory()` names.
	public static func realHomeFolder() -> URL {
		guard let entry = getpwuid(getuid()), let directory = entry.pointee.pw_dir else {
			return FileManager.default.homeDirectoryForCurrentUser
		}
		return URL(fileURLWithPath: String(cString: directory), isDirectory: true)
	}

	/// Application Support as this process sees it: inside the container when sandboxed.
	public static func sandboxApplicationSupportFolder() -> URL? {
		FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
	}
}
