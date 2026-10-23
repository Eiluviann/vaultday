import Foundation
import XCTest

/// The three entitlement sets under `Signing/` (docs/signing.md): ad hoc ⊂
/// Personal Team ⊂ full, except the folder exception the reduced sets add in
/// place of the App Group.
final class EntitlementVariantsTests: XCTestCase {
	/// `apps/apple/VaultdayTests/<file>` → `apps/apple/Signing`.
	private static let signing = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.appending(path: "Signing", directoryHint: .isDirectory)
	private static let folderException = "com.apple.security.temporary-exception.files.home-relative-path.read-write"

	func testTheReducedSetsDropOnlyWhatNeedsAProfile() throws {
		let full = try Self.entitlements("Full")
		let personalTeam = try Self.entitlements("PersonalTeam")
		let adHoc = try Self.entitlements("AdHoc")

		XCTAssertEqual(Set(full.keys).subtracting(personalTeam.keys), ["com.apple.security.application-groups"])
		XCTAssertEqual(Set(personalTeam.keys).subtracting(adHoc.keys), ["keychain-access-groups"])
		XCTAssertEqual(Set(personalTeam.keys).subtracting(full.keys), [Self.folderException])
		XCTAssertEqual(Set(adHoc.keys).subtracting(full.keys), [Self.folderException])
		for (key, value) in adHoc where key != Self.folderException {
			XCTAssertEqual(value as? NSObject, full[key] as? NSObject, key)
			XCTAssertEqual(value as? NSObject, personalTeam[key] as? NSObject, key)
		}
		XCTAssertEqual(personalTeam["keychain-access-groups"] as? NSObject, full["keychain-access-groups"] as? NSObject)
	}

	func testTheFolderExceptionIsTheLocalStateRoot() throws {
		for name in ["PersonalTeam", "AdHoc"] {
			XCTAssertEqual(try Self.entitlements(name)[Self.folderException] as? [String], ["/Library/Application Support/Vaultday/"], name)
		}
	}

	func testTheDefaultSigningIsAdHocWithTheReducedSet() throws {
		let xcconfig = try String(contentsOf: Self.signing.deletingLastPathComponent().appending(path: "Signing.xcconfig"), encoding: .utf8)
		XCTAssertTrue(xcconfig.contains("CODE_SIGN_IDENTITY = -\n"))
		XCTAssertTrue(xcconfig.contains("VAULTDAY_ENTITLEMENTS = Signing/AdHoc.entitlements\n"))
		let include = try XCTUnwrap(xcconfig.range(of: "#include? \"Signing.local.xcconfig\""))
		XCTAssertLessThan(try XCTUnwrap(xcconfig.range(of: "VAULTDAY_ENTITLEMENTS =")).lowerBound, include.lowerBound, "the local file overrides the defaults")
	}

	private static func entitlements(_ name: String) throws -> [String: Any] {
		let data = try Data(contentsOf: signing.appending(path: "\(name).entitlements"))
		let plist = try PropertyListSerialization.propertyList(from: data, format: nil)
		return try XCTUnwrap(plist as? [String: Any], "\(name).entitlements is not a dictionary")
	}
}
