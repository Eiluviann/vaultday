import Foundation
import XCTest
@testable import VaultdayCore

@MainActor
final class KeyboardTests: XCTestCase {
	private func repoKeymap() throws -> Keymap {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		return try Keymap.parse(text).get()
	}

	private func stroke(_ notation: String) -> KeyStroke {
		KeyStroke(notation: notation) ?? KeyStroke(key: "?")
	}

	func testTheRepositoryKeymapParsesAndKeepsHJKLForMovement() throws {
		let keymap = try repoKeymap()
		let movement: Set<KeyAction> = [.moveLeft, .moveDown, .moveUp, .moveRight]
		for (sequence, action) in keymap.sequences(for: .vim) where sequence.count == 1 && sequence[0].modifiers.isEmpty {
			if ["h", "j", "k", "l"].contains(sequence[0].key) {
				XCTAssertTrue(movement.contains(action), "bare \(sequence[0].key) is bound to \(action)")
			}
		}
		let paneJumps = keymap.sequences(for: .vim).filter { if case .focusPane = $0.1 { true } else { false } }
		XCTAssertEqual(paneJumps.map { $0.0.map(\.key).joined() }.sorted(), ["g0", "g1", "g2", "g3"])
	}

	func testVimPaneJumpsGoThroughAPendingStroke() throws {
		let dispatcher = KeyDispatcher(keymap: try repoKeymap(), scheme: .vim)
		XCTAssertEqual(dispatcher.handle(stroke("g")), .pending([stroke("g")]))
		XCTAssertEqual(dispatcher.pendingText, "g")
		XCTAssertEqual(dispatcher.handle(stroke("2")), .action(.focusPane(.inbox)))
		XCTAssertEqual(dispatcher.pendingText, "")

		XCTAssertEqual(dispatcher.handle(stroke("g")), .pending([stroke("g")]))
		XCTAssertEqual(dispatcher.handle(stroke("g")), .action(.moveFirst))
		XCTAssertEqual(dispatcher.handle(stroke("shift+g")), .action(.moveLast))
		XCTAssertEqual(dispatcher.handle(stroke("j")), .action(.moveDown))
	}

	func testABrokenPrefixStartsOverWithTheNewStroke() throws {
		let dispatcher = KeyDispatcher(keymap: try repoKeymap(), scheme: .vim)
		_ = dispatcher.handle(stroke("g"))
		XCTAssertEqual(dispatcher.handle(stroke("j")), .action(.moveDown))
		_ = dispatcher.handle(stroke("g"))
		XCTAssertEqual(dispatcher.handle(stroke("x")), .unbound)
		XCTAssertEqual(dispatcher.pending, [])
	}

	func testTheStandardSchemeUsesArrowsAndHasNoVimKeys() throws {
		let dispatcher = KeyDispatcher(keymap: try repoKeymap(), scheme: .standard)
		XCTAssertEqual(dispatcher.handle(stroke("down")), .action(.moveDown))
		XCTAssertEqual(dispatcher.handle(stroke("right")), .action(.moveRight))
		XCTAssertEqual(dispatcher.handle(stroke("return")), .action(.open))
		XCTAssertEqual(dispatcher.handle(stroke("j")), .unbound)
		XCTAssertEqual(dispatcher.handle(stroke("g")), .unbound)
		XCTAssertEqual(dispatcher.handle(stroke("ctrl+cmd+s")), .action(.toggleSidebar))
		dispatcher.setScheme(.vim)
		XCTAssertEqual(dispatcher.handle(stroke("j")), .action(.moveDown))
	}

	func testMalformedKeymapsAreRefusedWithALine() {
		let cases: [(String, Int)] = [
			("version: 1\nbindings:\n  - action: nope\n    vim: \"j\"\n", 3),
			("version: 1\nbindings:\n  - action: open\n    vim: \"hyper+x\"\n", 3),
			("version: 1\nbindings:\n  - action: open\n    vim: \"j\"\n  - action: move.down\n    vim: \"j\"\n", 5),
			("bindings:\n", 1),
			("version: 1\nkeys:\n", 2),
		]
		for (text, line) in cases {
			switch Keymap.parse(text) {
			case .success: XCTFail("accepted: \(text)")
			case let .failure(error): XCTAssertEqual(error.line, line, error.description)
			}
		}
	}

	func testFocusAndSelectionStayInRange() {
		let focus = PaneFocusModel()
		focus.setItemCount(3, in: .today)
		focus.apply(.moveUp)
		XCTAssertEqual(focus.selection(in: .today), 0)
		focus.apply(.moveDown)
		focus.apply(.moveDown)
		focus.apply(.moveDown)
		XCTAssertEqual(focus.selection(in: .today), 2)
		focus.apply(.moveFirst)
		XCTAssertEqual(focus.selection(in: .today), 0)
		focus.apply(.moveLast)
		focus.apply(.open)
		XCTAssertEqual(focus.opened, OpenedItem(pane: .today, index: 2))

		focus.apply(.moveLeft)
		XCTAssertEqual(focus.focusedPane, .sidebar)
		focus.apply(.moveLeft)
		XCTAssertEqual(focus.focusedPane, .sidebar, "no pane left of the sidebar")
		focus.apply(.focusPane(.sidePanel))
		focus.apply(.moveRight)
		XCTAssertEqual(focus.focusedPane, .sidePanel)
		XCTAssertNil(focus.selection(in: .inbox), "an empty pane has no selection")

		focus.setItemCount(1, in: .today)
		XCTAssertEqual(focus.selection(in: .today), 0, "a shrinking list keeps the selection inside it")
		focus.apply(.toggleSidebar)
		XCTAssertTrue(focus.isSidebarCollapsed)
	}

	func testWindowStateAndLinks() throws {
		let store = WindowStateStore(layout: AppGroupLayout(container: try TestFixtures.temporaryFolder(self, "Group")))
		XCTAssertEqual(store.load(), WindowState())
		store.save(WindowState(isSidebarCollapsed: true, focusedPane: .inbox))
		XCTAssertEqual(store.load(), WindowState(isSidebarCollapsed: true, focusedPane: .inbox))

		XCTAssertEqual(VaultdayLink(URL(string: "vaultday://story/WEB-142")!), .notHandledYet(host: "story", path: "/WEB-142"))
		XCTAssertNil(VaultdayLink(URL(string: "https://example.test")!))
	}
}
