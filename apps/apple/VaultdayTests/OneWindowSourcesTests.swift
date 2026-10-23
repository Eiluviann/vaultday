import Foundation
import XCTest

/// The app has one window: its sources declare the main `Window` scene, the
/// menu-bar extra and the mini tracker's panel, and open nothing else.
/// Setup, Settings and the sync check render inside the main window.
final class OneWindowSourcesTests: XCTestCase {
	/// `apps/apple/VaultdayTests/<file>` → `apps/apple/Vaultday`.
	private static let appSources = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.appending(path: "Vaultday", directoryHint: .isDirectory)

	func testOnlyTheMainWindowTheMenuBarExtraAndTheMiniTrackerExist() throws {
		let sources = try Self.sources()
		XCTAssertGreaterThan(sources.count, 20, "the app's sources were not found")
		let all = sources.values.joined(separator: "\n")

		XCTAssertEqual(Self.count(#"\bWindow\("#, in: all), 1, "one Window scene")
		XCTAssertEqual(Self.count(#"\bMenuBarExtra\s*\{"#, in: all), 1)
		XCTAssertEqual(Self.count(#"\bWindowGroup\b|\bDocumentGroup\b|\bUtilityWindow\b"#, in: all), 0)
		XCTAssertEqual(Self.count(#"\bSettings\s*\{|\bSettingsLink\b|\bopenSettings\b|showSettingsWindow"#, in: all), 0, "Settings is a sheet")
		XCTAssertEqual(Self.count(#"openWindow\(id:(?!\s*MainWindowModel\.sceneId)"#, in: all), 0, "openWindow reopens the main window only")
		XCTAssertEqual(Self.count(#"\bNSWindow\(|\bNSWindowController\(|\bNSAlert\(\)\.runModal"#, in: all), 0)

		XCTAssertEqual(Self.count(#"\.begin\(\)"#, in: sources["FolderPicker.swift"] ?? ""), 1, "the open panel floats only when no window is open")
		XCTAssertEqual(Self.count(#"beginSheetModal\(for:"#, in: sources["FolderPicker.swift"] ?? ""), 1, "the open panel is a sheet")

		let panelFiles = sources.filter { Self.count(#"\bNSPanel\("#, in: $0.value) > 0 }.keys.sorted()
		XCTAssertEqual(panelFiles, ["MiniTrackerController.swift"], "the mini tracker is the one panel")
	}

	func testTheCheckCatchesASecondWindow() {
		XCTAssertEqual(Self.count(#"openWindow\(id:(?!\s*MainWindowModel\.sceneId)"#, in: #"openWindow(id: "setup")"#), 1)
		XCTAssertEqual(Self.count(#"openWindow\(id:(?!\s*MainWindowModel\.sceneId)"#, in: "openWindow(id: MainWindowModel.sceneId)"), 0)
		XCTAssertEqual(Self.count(#"\bSettings\s*\{"#, in: "Settings {\n\tSettingsSheet(app: app)\n}"), 1)
		XCTAssertEqual(Self.count(#"\bWindow\("#, in: #"Window("Vaultday", id: "home") WindowReader(onWindow:)"#), 1)
	}

	/// Every Swift file under `Vaultday/`, by file name.
	private static func sources() throws -> [String: String] {
		let enumerator = try XCTUnwrap(FileManager.default.enumerator(at: appSources, includingPropertiesForKeys: nil))
		let files = enumerator.compactMap { $0 as? URL }.filter { $0.pathExtension == "swift" }
		return try Dictionary(uniqueKeysWithValues: files.map { ($0.lastPathComponent, try String(contentsOf: $0, encoding: .utf8)) })
	}

	private static func count(_ pattern: String, in source: String) -> Int {
		guard let expression = try? NSRegularExpression(pattern: pattern) else { return -1 }
		return expression.numberOfMatches(in: source, range: NSRange(source.startIndex..., in: source))
	}
}
