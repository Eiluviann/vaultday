import Foundation
import XCTest
@testable import VaultdayCore

/// The focus tree (window, pane, section, row, input), Esc one level up, the
/// sidebar's rows, the Today header as a section, `[` `]` on the focused
/// pane, and the design note's two keyboard storyboards, all without views.
@MainActor
final class FocusTreeTests: XCTestCase {
	private func repoKeymap() throws -> Keymap {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		return try Keymap.parse(text).get()
	}

	private func stroke(_ notation: String) -> KeyStroke {
		KeyStroke(notation: notation) ?? KeyStroke(key: "?")
	}

	/// A Home window's session over a day of three cards, a sidebar of six
	/// rows, a header of eight controls and an Inbox of two signals.
	private func makeSession(_ scheme: KeyScheme) throws -> (KeyboardSession, () -> [KeyAction]) {
		let keymap = try repoKeymap()
		let focus = PaneFocusModel(scheme: scheme)
		let session = KeyboardSession(dispatcher: KeyDispatcher(keymap: keymap, scheme: scheme), focus: focus, register: PlanRegister(plan: FakePlan()) { _ in })
		var actions: [KeyAction] = []
		session.onAction = { actions.append($0) }
		focus.setItemCount(3, in: .today)
		focus.setItemCount(6, in: .sidebar)
		focus.setItemCount(2, in: .inbox)
		focus.setControlCount(8, in: .dayHeader)
		return (session, { actions })
	}

	private func type(_ notation: String, into session: KeyboardSession) {
		for part in notation.split(separator: " ") {
			session.handle(stroke(String(part)))
		}
	}

	// MARK: Panel numbering

	func testPanelsCountFromTheSidebar() throws {
		let (session, _) = try makeSession(.vim)
		for (keys, pane) in [("g 1", Pane.sidebar), ("g 2", .today), ("g 3", .inbox), ("g 4", .sidePanel)] {
			type(keys, into: session)
			XCTAssertEqual(session.focus.focusedPane, pane, keys)
		}
		type("g", into: session)
		let numbers = session.dispatcher.completions.compactMap { completion -> String? in
			if case let .focusPane(pane) = completion.action { "\(pane):\(completion.next.capText)" } else { nil }
		}
		XCTAssertEqual(numbers, ["sidebar:1", "today:2", "inbox:3", "sidePanel:4"], "each pane shows its number while g is pending")
		XCTAssertEqual(session.dispatcher.handle(stroke("0")), .unbound, "g0 is free")
		let standard = try makeSession(.standard).0
		type("ctrl+1", into: standard)
		XCTAssertEqual(standard.focus.focusedPane, .sidebar)
		type("ctrl+4", into: standard)
		XCTAssertEqual(standard.focus.focusedPane, .sidePanel)
	}

	// MARK: Esc, one level up

	func testEscMovesOneLevelUpUnderVim() throws {
		let (session, _) = try makeSession(.vim)
		let focus = session.focus
		type("g 2 j j", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 1))

		focus.enterInput(FocusedInput(name: "quick-add", pane: .today))
		XCTAssertEqual(focus.level, .input(FocusedInput(name: "quick-add", pane: .today)))
		type("escape", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 1), "input to its pane, at the row it left")
		type("escape", into: session)
		XCTAssertEqual(focus.level, .pane(.today), "row to the container")
		type("escape", into: session)
		XCTAssertEqual(focus.level, .pane(.today), "a pane is as high as Esc goes")
		type("j", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 0), "hjkl act at once after Esc")
	}

	func testEscUnderStandardStopsAtTheRow() throws {
		let (session, _) = try makeSession(.standard)
		let focus = session.focus
		type("ctrl+2 down", into: session)
		focus.enterInput(FocusedInput(name: "quick-add", pane: .today))
		type("escape", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 1))
		type("escape", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 1), "a Standard row is the floor")
	}

	func testEscClosesTransientLayersBeforeMoving() throws {
		let (session, _) = try makeSession(.vim)
		var openLayers = 1
		session.dismissTransient = {
			guard openLayers > 0 else { return false }
			openLayers -= 1
			return true
		}
		type("g 2 j escape", into: session)
		XCTAssertEqual(session.focus.level, .row(.today, index: 0), "the first Esc closed the layer")
		type("escape", into: session)
		XCTAssertEqual(session.focus.level, .pane(.today))
	}

	func testALeftInputGivesTheKeysBackOnlyByName() {
		let focus = PaneFocusModel(scheme: .vim)
		focus.enterInput(FocusedInput(name: "browse-query", pane: .today))
		focus.leaveInput(named: "quick-add")
		XCTAssertNotNil(focus.input, "another field leaving does not clear this one")
		focus.leaveInput(named: "browse-query")
		XCTAssertNil(focus.input)
	}

	// MARK: The Today header as a section

	func testTheHeaderIsASectionAboveTheCards() throws {
		let (session, actions) = try makeSession(.vim)
		let focus = session.focus
		type("g 2 j k", into: session)
		XCTAssertEqual(focus.level, .section(.dayHeader, control: 0), "k above the first card enters the header")
		XCTAssertNil(focus.selection(in: .today), "no card is selected while the header holds the keys")
		type("l l l", into: session)
		XCTAssertEqual(focus.control(in: .dayHeader), 3)
		type("l l l l l l", into: session)
		XCTAssertEqual(focus.control(in: .dayHeader), 7, "l stops at the last control")
		XCTAssertEqual(focus.focusedPane, .today, "h and l walk the header, they do not leave the pane")
		type("return", into: session)
		XCTAssertEqual(actions().last, .open, "⏎ goes to the app, which presses the control")
		XCTAssertEqual(focus.level, .section(.dayHeader, control: 7))
		type("j", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 0), "j comes back down to the first card")
		type("k escape", into: session)
		XCTAssertEqual(focus.level, .pane(.today), "Esc leaves the header for the pane")
		type("k", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 2), "from the container k is still the last card")
	}

	func testTheHeaderUnderStandardAndOverAnEmptyDay() throws {
		let (session, _) = try makeSession(.standard)
		let focus = session.focus
		type("ctrl+2 up right", into: session)
		XCTAssertEqual(focus.level, .section(.dayHeader, control: 1))
		type("escape", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 0), "Standard lands on the first card")

		focus.setItemCount(0, in: .today)
		type("up", into: session)
		XCTAssertEqual(focus.level, .section(.dayHeader, control: 1), "an empty day still reaches its header, at the control it left")
		focus.setControlCount(0, in: .dayHeader)
		XCTAssertNil(focus.section, "a header that shows no controls gives the keys back")
	}

	// MARK: The sidebar's rows

	func testTheSidebarLandsOnItsCurrentRow() throws {
		let (session, actions) = try makeSession(.vim)
		let focus = session.focus
		focus.setCurrentRow(2, in: .sidebar)
		type("g 1", into: session)
		XCTAssertEqual(focus.level, .row(.sidebar, index: 2), "g1 lands on the current row, not the container")
		type("j j", into: session)
		XCTAssertEqual(focus.selection(in: .sidebar), 4)
		type("return", into: session)
		XCTAssertEqual(actions().last, .open)
		XCTAssertEqual(focus.opened, OpenedItem(pane: .sidebar, index: 4))
		type("escape", into: session)
		XCTAssertEqual(focus.level, .pane(.sidebar))
		type("l g 1", into: session)
		XCTAssertEqual(focus.level, .row(.sidebar, index: 4), "the row is kept while the pane is away")
	}

	// MARK: `[` and `]`

	func testBracketsActOnTheFocusedPaneAndFallBackToTheOnlyTabbedPane() throws {
		let vim = KeyDispatcher(keymap: try repoKeymap(), scheme: .vim)
		for pane in [Pane.sidebar, .today, .inbox, .sidePanel] {
			XCTAssertEqual(vim.handle(stroke("]"), in: pane), .action(.nextSidePanelTab), "] from \(pane)")
		}
		XCTAssertEqual(vim.handle(stroke("]"), in: .storyPanel, screen: Pane.storyOrder), .action(.nextStoryPanelTab))
		XCTAssertEqual(vim.handle(stroke("["), in: .today, screen: Pane.storyOrder), .action(.previousStoryPanelTab), "the note falls back to the story panel")

		// A pane that gets its own `]` wins there, and only there.
		let header = "version: 1\nbindings:\n"
		let text = header + """
		  - action: side-panel.next
		    label: "Next side panel tab"
		    pane: "side-panel"
		    fallback: true
		    vim: "]"
		  - action: browse.next-group
		    label: "Next group"
		    pane: "today"
		    vim: "]"
		"""
		let dispatcher = KeyDispatcher(keymap: try Keymap.parse(text).get(), scheme: .vim)
		XCTAssertEqual(dispatcher.handle(stroke("]"), in: .today), .action(.nextGroup), "the focused pane's meaning wins")
		XCTAssertEqual(dispatcher.handle(stroke("]"), in: .inbox), .action(.nextSidePanelTab))
		XCTAssertEqual(dispatcher.handle(stroke("]"), in: .inbox, screen: Pane.storyOrder), .unbound, "no fallback to a pane off screen")
	}

	func testTwoFallbacksOnOneScreenAreRefused() {
		let header = "version: 1\nbindings:\n"
		let text = header + """
		  - action: side-panel.next
		    label: "Next side panel tab"
		    pane: "side-panel"
		    fallback: true
		    vim: "]"
		  - action: browse.next-group
		    label: "Next group"
		    pane: "inbox"
		    fallback: true
		    vim: "]"
		"""
		XCTAssertThrowsError(try Keymap.parse(text).get(), "two tabbed panes on Home cannot both own `]` from elsewhere")
		let unscoped = header + "  - action: open\n    label: \"Open\"\n    fallback: true\n    vim: \"x\"\n"
		XCTAssertThrowsError(try Keymap.parse(unscoped).get(), "a fallback names the pane it falls back to")
	}

	// MARK: The key left of 1

	func testAllWorkspacesIsTheKeyLeftOfOne() throws {
		let keymap = try repoKeymap()
		for scheme in KeyScheme.allCases {
			let dispatcher = KeyDispatcher(keymap: keymap, scheme: scheme)
			XCTAssertEqual(dispatcher.handle(KeyStroke(key: KeyStroke.leftOfOneKey, modifiers: .command)), .action(.showAllWorkspaces))
			XCTAssertEqual(dispatcher.handle(stroke("cmd+1")), .action(.showWorkspace(1)))
		}
		XCTAssertEqual(KeyStroke(key: KeyStroke.leftOfOneKey, modifiers: .command).capText, "⌘\(KeyStroke.leftOfOneCap)")
	}

	// MARK: The design note's storyboards

	/// Vim motions on the JTI Home, six steps: the card current in its washed
	/// pane; `\` moves focus to the card's menu; `g3` moves wash and caps to
	/// the Inbox; a pending `g` shows the pane numbers, lights `gb` and `gl`
	/// and prints the showcmd; ⏎ opens the selected signal's preview.
	func testTheDesignNotesVimStoryboard() throws {
		let (session, actions) = try makeSession(.vim)
		let focus = session.focus
		session.selectedTaskId = { focus.selection(in: .today).map { "task-\($0 + 1)" } }
		session.selectedSignalId = { focus.selection(in: .inbox).map { "signal-\($0 + 1)" } }

		// 1. Today washed, its first card current.
		type("g 2 j", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 0))

		// 2. `\`: the menu takes the focus, the card's caps leave.
		type("\\", into: session)
		XCTAssertTrue(session.showsItemMenu(in: .today))

		// 3. `g3`: wash and caps move to the Inbox, the menu closes.
		type("g 3", into: session)
		XCTAssertEqual(focus.focusedPane, .inbox)
		XCTAssertFalse(session.showsItemMenu)
		type("j", into: session)
		XCTAssertEqual(focus.level, .row(.inbox, index: 0))

		// 4. `g` pending: pane numbers, `gb` and `gl` lit, the showcmd.
		type("g", into: session)
		XCTAssertTrue(session.dispatcher.showsPaneNumbers)
		XCTAssertEqual(session.dispatcher.pendingText, "g")
		let lit = Set(session.dispatcher.completions.map(\.action))
		XCTAssertTrue(lit.isSuperset(of: [.showBrowse, .showDayLog]))

		// 5. Esc drops the `g` and nothing else; the signal stays current.
		type("escape", into: session)
		XCTAssertEqual(session.dispatcher.pendingText, "")
		XCTAssertEqual(focus.level, .row(.inbox, index: 0))

		// 6. ⏎ opens the signal's preview.
		type("return", into: session)
		XCTAssertEqual(focus.opened, OpenedItem(pane: .inbox, index: 0))
		XCTAssertEqual(actions().last, .open)
	}

	/// Standard on the JTI Home, six steps: the ring on the card itself;
	/// the arrows cycle cards; Tab is the system's, onto the card's menu key,
	/// and ⌃⏎ opens the menu; → crosses into the Inbox onto its first card;
	/// ⏎ opens the preview. Nothing is ever pending.
	func testTheDesignNotesStandardStoryboard() throws {
		let (session, actions) = try makeSession(.standard)
		let focus = session.focus
		session.selectedTaskId = { focus.selection(in: .today).map { "task-\($0 + 1)" } }

		// 1. The ring on the first card.
		type("ctrl+2", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 0))

		// 2. ↓ ↓ ↑: the arrows walk the cards.
		type("down down up", into: session)
		XCTAssertEqual(focus.level, .row(.today, index: 1))

		// 3. Tab is AppKit's focus key, not a binding.
		XCTAssertEqual(session.handle(stroke("tab")), .unbound)

		// 4. ⌃⏎: the card's menu.
		type("ctrl+return", into: session)
		XCTAssertTrue(session.showsItemMenu(in: .today))
		type("escape", into: session)
		XCTAssertFalse(session.showsItemMenu)

		// 5. →: the Inbox, on its first card.
		type("right", into: session)
		XCTAssertEqual(focus.level, .row(.inbox, index: 0))

		// 6. ⏎: the preview.
		type("return", into: session)
		XCTAssertEqual(focus.opened, OpenedItem(pane: .inbox, index: 0))
		XCTAssertEqual(actions().last, .open)
		XCTAssertEqual(session.dispatcher.pendingText, "")
	}
}
