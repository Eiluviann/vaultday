import Foundation
import XCTest
@testable import VaultdayCore

/// The bindings of Browse, the Day log and Peek come from Keymap.yaml.
@MainActor
final class BrowseKeyboardTests: XCTestCase {
	private func repoKeymap() throws -> Keymap {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		return try Keymap.parse(text).get()
	}

	private func stroke(_ notation: String) -> KeyStroke {
		KeyStroke(notation: notation) ?? KeyStroke(key: "?")
	}

	func testGbAndGlGoThroughThePendingG() throws {
		let dispatcher = KeyDispatcher(keymap: try repoKeymap(), scheme: .vim)

		XCTAssertEqual(dispatcher.handle(stroke("g")), .pending([stroke("g")]))
		XCTAssertEqual(dispatcher.handle(stroke("b")), .action(.showBrowse))
		XCTAssertEqual(dispatcher.handle(stroke("g")), .pending([stroke("g")]))
		XCTAssertEqual(dispatcher.handle(stroke("l")), .action(.showDayLog))
		XCTAssertEqual(dispatcher.label(for: .showBrowse), "g b")
		XCTAssertEqual(dispatcher.label(for: .showDayLog), "g l")
	}

	func testBrowseKeysAndPeekUnderBothSchemes() throws {
		let keymap = try repoKeymap()
		let vim = KeyDispatcher(keymap: keymap, scheme: .vim)
		XCTAssertEqual(vim.handle(stroke("/")), .action(.focusBrowseQuery))
		XCTAssertEqual(vim.handle(stroke("shift+k")), .action(.peek))
		XCTAssertEqual(vim.handle(stroke("shift+}")), .action(.nextGroup))
		XCTAssertEqual(vim.handle(stroke("shift+{")), .action(.previousGroup))
		XCTAssertEqual(vim.handle(stroke("n")), .action(.nextMatch))
		XCTAssertEqual(vim.handle(stroke("shift+n")), .action(.previousMatch))
		XCTAssertEqual(vim.handle(stroke("z")), .pending([stroke("z")]))
		XCTAssertEqual(vim.handle(stroke("a")), .action(.toggleFold))
		XCTAssertEqual(vim.handle(stroke("k")), .action(.moveUp), "a bare k is still only movement")

		let standard = KeyDispatcher(keymap: keymap, scheme: .standard)
		XCTAssertEqual(standard.handle(stroke("shift+cmd+b")), .action(.showBrowse))
		XCTAssertEqual(standard.handle(stroke("shift+cmd+l")), .action(.showDayLog))
		XCTAssertEqual(standard.handle(stroke("cmd+y")), .action(.peek))
		XCTAssertEqual(standard.handle(stroke("cmd+f")), .action(.focusBrowseQuery))
		XCTAssertEqual(standard.handle(stroke("b")), .unbound, "the standard scheme has no bare letters")
		XCTAssertNil(standard.label(for: .toggleFold), "folding by key is a vim binding")
		XCTAssertEqual(standard.label(for: .peek), "⌘Y")
	}

	func testTheRailTabsHaveTheirNumbers() throws {
		let keymap = try repoKeymap()
		for scheme in KeyScheme.allCases {
			let dispatcher = KeyDispatcher(keymap: keymap, scheme: scheme)
			for place in 1 ... 7 {
				XCTAssertEqual(dispatcher.handle(stroke("alt+\(place)")), .action(.showSidePanelTab(place)), "\(scheme) \(place)")
			}
			XCTAssertEqual(dispatcher.handle(stroke("alt+8")), .unbound)
		}
		XCTAssertEqual(KeyAction(name: "side-panel.tab.3"), .showSidePanelTab(3))
		XCTAssertNil(KeyAction(name: "side-panel.tab.0"))
	}

	func testFocusFollowsTheNewActions() {
		let focus = PaneFocusModel(focusedPane: .inbox)
		focus.setItemCount(4, in: .today)
		focus.apply(.showBrowse)
		XCTAssertEqual(focus.focusedPane, .today)
		focus.apply(.showDayLog)
		XCTAssertEqual(focus.focusedPane, .sidePanel)
		focus.apply(.focusPane(.today))
		focus.select(1, in: .today)
		focus.apply(.nextMatch)
		XCTAssertEqual(focus.selection(in: .today), 1, "n jumps by the query's matches, which the app knows, not by a line")
		focus.apply(.showSidePanelTab(4))
		XCTAssertEqual(focus.focusedPane, .sidePanel)
	}
}
