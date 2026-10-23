import AppKit
import VaultdayCore
import XCTest

/// The open panel is a sheet of the one main window, or of the sheet over
/// it, and never of a popover: the Add and Restore popovers close as the
/// panel opens, and a panel attached to a closing popover goes with it.
@MainActor
final class FolderPickerHostTests: XCTestCase {
	func testThePanelAttachesToTheMainWindowNotToAPopoverOverIt() throws {
		let main = window(identifier: "\(MainWindowModel.sceneId)-AppWindow-1")
		let popover = window(identifier: nil)
		main.addChildWindow(popover, ordered: .above)
		addTeardownBlock { main.removeChildWindow(popover) }

		XCTAssertIdentical(FolderPicker.sheetHost(in: [popover, main]), main)
	}

	func testThePanelAttachesToTheSheetOverTheMainWindow() throws {
		let main = window(identifier: "\(MainWindowModel.sceneId)-AppWindow-1")
		let settings = window(identifier: nil, isShown: false)
		main.beginSheet(settings)
		addTeardownBlock { main.endSheet(settings) }
		try XCTSkipUnless(main.attachedSheet === settings, "no window server to attach a sheet")

		XCTAssertIdentical(FolderPicker.sheetHost(in: [main, settings]), settings)
	}

	func testWithTheMainWindowClosedThePanelFloats() {
		let closed = window(identifier: "\(MainWindowModel.sceneId)-AppWindow-1", isShown: false)
		let tracker = window(identifier: nil)

		XCTAssertNil(FolderPicker.sheetHost(in: [closed, tracker]))
	}

	private func window(identifier: String?, isShown: Bool = true) -> NSWindow {
		let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 120), styleMask: [.titled], backing: .buffered, defer: true)
		window.isReleasedWhenClosed = false
		window.identifier = identifier.map { NSUserInterfaceItemIdentifier($0) }
		if isShown { window.orderFront(nil) }
		addTeardownBlock { window.orderOut(nil) }
		return window
	}
}
