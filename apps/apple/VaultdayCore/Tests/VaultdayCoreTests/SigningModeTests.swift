import Foundation
import Security
import XCTest
@testable import VaultdayCore

/// The storage a build runs on follows from the entitlements it claims, one
/// entitlement at a time (docs/signing.md).
final class SigningModeTests: XCTestCase {
	private let group = "group.com.eiluviann.vaultday"
	private let teamGroup = "ABCDE12345.com.eiluviann.vaultday"

	func testAPaidTeamBuildRunsOnTheAppGroupAndSynchronises() {
		let entitlements = SigningEntitlements(isSandboxed: true, applicationGroups: [group], keychainAccessGroups: [teamGroup])
		let mode = SigningMode.resolve(entitlements: entitlements, appGroup: group)
		XCTAssertEqual(mode, .appGroup(group))
		XCTAssertTrue(mode.isSynchronising)
		XCTAssertNil(mode.localStateFolderName)
		XCTAssertNil(mode.notice(stateFolder: URL(fileURLWithPath: "/tmp")))
	}

	func testAPersonalTeamBuildWithItsOwnIdentifiersRunsOnItsTeamAppGroup() {
		// Signing.personal-team.xcconfig: the App Group in Info.plist is the team-prefixed one it claims.
		let personalGroup = "ABCDE12345.com.eiluviann.vaultday"
		let entitlements = SigningEntitlements(isSandboxed: true, applicationGroups: [personalGroup], keychainAccessGroups: ["ABCDE12345.com.eiluviann.vaultday.personal"])
		XCTAssertEqual(SigningMode.resolve(entitlements: entitlements, appGroup: personalGroup), .appGroup(personalGroup))
		XCTAssertEqual(SigningMode.resolve(entitlements: entitlements, appGroup: group), .teamKeychain(accessGroup: "ABCDE12345.com.eiluviann.vaultday.personal"), "a build whose Info.plist names another group does not take this one")
	}

	func testAPersonalTeamBuildWithoutTheAppGroupKeepsTheTeamKeychainGroupWithoutSync() {
		let entitlements = SigningEntitlements(isSandboxed: true, applicationGroups: [], keychainAccessGroups: [teamGroup])
		let mode = SigningMode.resolve(entitlements: entitlements, appGroup: group)
		XCTAssertEqual(mode, .teamKeychain(accessGroup: teamGroup))
		XCTAssertFalse(mode.isSynchronising)
		XCTAssertEqual(mode.localStateFolderName, teamGroup)
		XCTAssertEqual(mode.notice(stateFolder: URL(fileURLWithPath: "/state"))?.contains("/state"), true)
	}

	func testInfoPlistsKeychainGroupWinsOverTheEntitlementsOrder() {
		let entitlements = SigningEntitlements(isSandboxed: true, applicationGroups: [], keychainAccessGroups: ["com.apple.token", teamGroup])
		XCTAssertEqual(SigningMode.resolve(entitlements: entitlements, appGroup: group, preferredKeychainGroup: teamGroup), .teamKeychain(accessGroup: teamGroup))
		XCTAssertEqual(SigningMode.resolve(entitlements: entitlements, appGroup: group, preferredKeychainGroup: "not-claimed"), .teamKeychain(accessGroup: "com.apple.token"))
	}

	func testOnlyAMissingKeychainMeansFallingBackToMemory() {
		XCTAssertTrue(SecItemKeychainBackend.meansNoKeychain(errSecNoSuchKeychain))
		XCTAssertTrue(SecItemKeychainBackend.meansNoKeychain(errSecNotAvailable))
		for status in [errSecInteractionNotAllowed, errSecUserCanceled, errSecAuthFailed, errSecItemNotFound, errSecSuccess] {
			XCTAssertFalse(SecItemKeychainBackend.meansNoKeychain(status), "\(status)")
		}
	}

	func testAnotherAppGroupOrAWildcardKeychainGroupDoesNotCount() {
		let entitlements = SigningEntitlements(isSandboxed: true, applicationGroups: ["group.other"], keychainAccessGroups: ["ABCDE12345.*", ""])
		XCTAssertEqual(SigningMode.resolve(entitlements: entitlements, appGroup: group), .loginKeychain)
	}

	func testAnAdHocOrUnsignedBuildUsesTheLoginKeychain() {
		for isSandboxed in [true, false] {
			let entitlements = SigningEntitlements(isSandboxed: isSandboxed, applicationGroups: [], keychainAccessGroups: [])
			let mode = SigningMode.resolve(entitlements: entitlements, appGroup: group)
			XCTAssertEqual(mode, .loginKeychain)
			XCTAssertFalse(mode.isSynchronising)
			XCTAssertEqual(mode.localStateFolderName, SigningMode.loginKeychainScope)
			XCTAssertNotNil(mode.notice(stateFolder: URL(fileURLWithPath: "/state")))
		}
	}

	func testThisTestRunnerClaimsNoAppGroup() {
		// The package tests run unsigned or ad hoc: no App Group, so the app's code path would degrade.
		XCTAssertEqual(SigningMode.resolve(entitlements: .ofThisProcess(), appGroup: group), .loginKeychain)
	}

	func testTheLocalRootPrefersTheRealHomeThenTheContainer() {
		let home = URL(fileURLWithPath: "/Users/someone", isDirectory: true)
		let container = URL(fileURLWithPath: "/Users/someone/Library/Containers/com.eiluviann.vaultday/Data/Library/Application Support", isDirectory: true)
		XCTAssertEqual(
			AppGroupLayout.localStateRootCandidates(realHome: home, applicationSupport: container).map(\.path),
			["/Users/someone/Library/Application Support/Vaultday", "/Users/someone/Library/Containers/com.eiluviann.vaultday/Data/Library/Application Support/Vaultday"],
		)
		// Unsandboxed, both name the same folder.
		let unsandboxed = home.appending(path: "Library/Application Support", directoryHint: .isDirectory)
		XCTAssertEqual(AppGroupLayout.localStateRootCandidates(realHome: home, applicationSupport: unsandboxed).count, 1)
	}

	func testTheFirstWritableRootWinsAndAnUnwritableOneIsSkipped() throws {
		let scratch = try TestFixtures.temporaryFolder(self)
		let locked = scratch.appending(path: "locked", directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: locked, withIntermediateDirectories: true)
		try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: locked.path)
		addTeardownBlock { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: locked.path) }
		let blocked = locked.appending(path: "Vaultday", directoryHint: .isDirectory)
		let open = scratch.appending(path: "open/Vaultday", directoryHint: .isDirectory)

		XCTAssertEqual(AppGroupLayout.firstWritableRoot([blocked, open]), open)
		XCTAssertTrue(FileManager.default.fileExists(atPath: open.path))
		XCTAssertNil(AppGroupLayout.firstWritableRoot([blocked]))
	}

	func testALocalLayoutKeepsStatePerKeychainAndOneAPIFolder() throws {
		let root = URL(fileURLWithPath: "/Users/someone/Library/Application Support/Vaultday", isDirectory: true)
		let login = AppGroupLayout.local(root: root, folderName: SigningMode.loginKeychainScope)
		let team = AppGroupLayout.local(root: root, folderName: teamGroup)
		XCTAssertEqual(login.appStateFolder.path, "/Users/someone/Library/Application Support/Vaultday/login-keychain/AppState")
		XCTAssertNotEqual(login.appStateFolder, team.appStateFolder)
		XCTAssertEqual(login.apiTokenFile.path, "/Users/someone/Library/Application Support/Vaultday/API/token")
		XCTAssertEqual(team.apiPortFile.path, "/Users/someone/Library/Application Support/Vaultday/API/port")
	}

	func testALocalEnvironmentPersistsKeysAcrossARelaunch() throws {
		let root = try TestFixtures.temporaryFolder(self)
		let keychain = InMemoryKeychainBackend()
		let layout = AppGroupLayout.local(root: root, folderName: SigningMode.loginKeychainScope)
		let first = try CoreEnvironment.inMemory(container: layout.container, keychain: keychain, signingMode: .loginKeychain)
		let token = try LocalAPICredentials.prepareToken(secrets: first.secrets, layout: first.layout)
		XCTAssertNotNil(first.signingNotice)

		let relaunched = try CoreEnvironment.inMemory(container: layout.container, keychain: keychain, signingMode: .loginKeychain)
		XCTAssertEqual(try LocalAPICredentials.prepareToken(secrets: relaunched.secrets, layout: relaunched.layout), token)
	}
}
