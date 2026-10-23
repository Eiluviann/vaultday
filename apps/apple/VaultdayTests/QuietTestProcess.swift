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
/// window of its own.
@MainActor
final class QuietTestProcessTests: XCTestCase {
	func testTheTestProcessCannotActivateOrShowAWindow() {
		XCTAssertEqual(NSApplication.shared.activationPolicy(), .prohibited)
		XCTAssertEqual(NSApplication.shared.windows.filter(\.isVisible), [])
	}
}
