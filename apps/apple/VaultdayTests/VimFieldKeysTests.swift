import AppKit
import Carbon.HIToolbox
import XCTest

/// Keys the Vim field reads before AppKit does, on keyboard layouts where a
/// key waits for the next one (a dead key): in Normal mode such a key is a
/// command, so it must never start AppKit's composing over the block cursor.
final class VimFieldKeysTests: XCTestCase {
	/// U.S. International – PC: ` ' " ^ ~ are dead keys on an ANSI keyboard.
	private func usInternationalLayout() throws -> CFData {
		let filter = [kTISPropertyInputSourceID: "com.apple.keylayout.USInternational-PC"] as CFDictionary
		let sources = try XCTUnwrap(TISCreateInputSourceList(filter, true)?.takeRetainedValue() as? [TISInputSource])
		let source = try XCTUnwrap(sources.first, "the layout ships with macOS")
		let property = try XCTUnwrap(TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData))
		return Unmanaged<CFData>.fromOpaque(property).takeUnretainedValue()
	}

	func testADeadKeyReadsAsTheCharacterItsCapShows() throws {
		let layout = try usInternationalLayout()
		XCTAssertNil(VimText.typedCharacter(keyCode: UInt16(kVK_ANSI_6), modifierFlags: .shift, layoutData: layout), "the premise: ⇧6 is a dead key on this layout")
		XCTAssertEqual(VimText.spacingCharacter(keyCode: UInt16(kVK_ANSI_6), modifierFlags: .shift, layoutData: layout), "^")
		XCTAssertEqual(VimText.spacingCharacter(keyCode: UInt16(kVK_ANSI_Grave), modifierFlags: [], layoutData: layout), "`")
		XCTAssertEqual(VimText.spacingCharacter(keyCode: UInt16(kVK_ANSI_Quote), modifierFlags: .shift, layoutData: layout), "\"")
		XCTAssertEqual(VimText.spacingCharacter(keyCode: UInt16(kVK_ANSI_W), modifierFlags: [], layoutData: layout), "w")
	}
}
