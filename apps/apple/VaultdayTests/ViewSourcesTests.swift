import Foundation
import XCTest

/// Views never touch the core's generated FFI types: every type the UniFFI
/// generator declares is looked for, as a whole word, in the app's view
/// sources (`Vaultday/Views/`). Models in VaultdayCore map them into plain values.
final class ViewSourcesTests: XCTestCase {
	/// `apps/apple/VaultdayTests/<file>` → `apps/apple`.
	private static let appleRoot = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
	private static let generated = appleRoot.appending(path: "VaultdayCore/Sources/VaultdayCore/Generated/VaultdayCore.swift")
	private static let views = appleRoot.appending(path: "Vaultday/Views", directoryHint: .isDirectory)

	func testViewSourcesNameNoGeneratedType() throws {
		let names = try Self.generatedTypeNames()
		XCTAssertGreaterThan(names.count, 20, "the generated bindings were not found; run scripts/build-xcframework.sh")
		XCTAssertTrue(names.isSuperset(of: ["TodayView", "WorkspaceHandle", "CoreError", "SecretStorePort", "SecretItem"]))

		let viewFiles = try FileManager.default.contentsOfDirectory(at: Self.views, includingPropertiesForKeys: nil)
			.filter { $0.pathExtension == "swift" }
		XCTAssertFalse(viewFiles.isEmpty)
		for file in viewFiles {
			let source = try String(contentsOf: file, encoding: .utf8)
			let words = Self.identifiers(in: source)
			let leaked = names.intersection(words).sorted()
			XCTAssertEqual(leaked, [], "\(file.lastPathComponent) names generated FFI types")
			XCTAssertFalse(source.contains("VaultdayCoreFFI"), "\(file.lastPathComponent) imports the FFI module")
		}
	}

	func testTheCheckCatchesALeak() {
		let words = Self.identifiers(in: "let view: TodayView = today(workspace: handle)")
		XCTAssertTrue(words.contains("TodayView"))
		XCTAssertFalse(Self.identifiers(in: "struct TodayViewer {}").contains("TodayView"))
	}

	/// Type names the generator declares (`public struct X`, `open class X`, …).
	private static func generatedTypeNames() throws -> Set<String> {
		let source = try String(contentsOf: generated, encoding: .utf8)
		let pattern = try NSRegularExpression(pattern: #"^(?:public\s+|open\s+)?(?:final\s+)?(?:struct|enum|class|protocol)\s+(\w+)"#, options: .anchorsMatchLines)
		let range = NSRange(source.startIndex..., in: source)
		return Set(pattern.matches(in: source, range: range).compactMap { match in
			Range(match.range(at: 1), in: source).map { String(source[$0]) }
		})
	}

	private static func identifiers(in source: String) -> Set<String> {
		Set(source.split(whereSeparator: { !($0.isLetter || $0.isNumber || $0 == "_") }).map(String.init))
	}
}
