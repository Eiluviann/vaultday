import AppKit
import XCTest

/// The bundle's principal class (`NSPrincipalClass` in project.yml), made
/// once by XCTest before any test runs: the xctest process may render
/// hosting views off screen but never shows up in the Dock, opens a window
/// or takes focus from whatever the developer is doing (docs/building.md,
/// "Quiet tests").
@objc(VaultdayQuietTestProcess)
final class QuietTestProcess: NSObject {
	override init() {
		super.init()
		MainActor.assumeIsolated {
			_ = NSApplication.shared.setActivationPolicy(.prohibited)
		}
	}
}

/// The process stays quiet: prohibited, so it cannot activate, and with no
/// window of its own. A test that needs a window on screen belongs in
/// `VaultdayWindowTests`, which runs on CI only.
@MainActor
final class QuietTestProcessTests: XCTestCase {
	/// `apps/apple/VaultdayTests/<file>` → `apps/apple/VaultdayTests`.
	private static let testSources = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
	/// Calls that put a window on screen or take focus.
	private static let windowCalls = #"\.(orderFront|orderFrontRegardless|makeKeyAndOrderFront|makeKey|beginSheet|runModal|activate)\("#

	func testTheTestProcessCannotActivateOrShowAWindow() {
		// A plain xctest process starts out prohibited anyway, so the policy
		// alone would not show that the principal class is wired up.
		XCTAssertEqual(Bundle(for: Self.self).object(forInfoDictionaryKey: "NSPrincipalClass") as? String, "VaultdayQuietTestProcess")
		XCTAssertTrue(Bundle(for: Self.self).principalClass == QuietTestProcess.self)
		XCTAssertEqual(NSApplication.shared.activationPolicy(), .prohibited)
		XCTAssertEqual(NSApplication.shared.windows.filter(\.isVisible), [])
	}

	func testNoAppTestPutsAWindowOnScreen() throws {
		let enumerator = try XCTUnwrap(FileManager.default.enumerator(at: Self.testSources, includingPropertiesForKeys: nil))
		let files = enumerator.compactMap { $0 as? URL }.filter { $0.pathExtension == "swift" && $0.lastPathComponent != "QuietTestProcess.swift" }
		XCTAssertGreaterThan(files.count, 3, "the app tests were not found under \(Self.testSources.path)")
		for file in files {
			let source = try String(contentsOf: file, encoding: .utf8)
			XCTAssertEqual(Self.countWindowCalls(in: source), 0, "\(file.lastPathComponent) puts a window on screen; move it to VaultdayWindowTests")
		}
	}

	func testTheCheckCatchesAWindowOnScreen() {
		XCTAssertEqual(Self.countWindowCalls(in: "window.orderFront(nil)"), 1)
		XCTAssertEqual(Self.countWindowCalls(in: "main.beginSheet(settings)"), 1)
		XCTAssertEqual(Self.countWindowCalls(in: "NSApp.activate(ignoringOtherApps: true)"), 1)
		XCTAssertEqual(Self.countWindowCalls(in: "let host = NSHostingView(rootView: view)"), 0)
		XCTAssertEqual(Self.countWindowCalls(in: ##"Self.count(#"beginSheetModal\(for:"#, in: source)"##), 0)
	}

	private static func countWindowCalls(in source: String) -> Int {
		guard let expression = try? NSRegularExpression(pattern: windowCalls) else { return -1 }
		return expression.numberOfMatches(in: source, range: NSRange(source.startIndex..., in: source))
	}
}
