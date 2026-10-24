import XCTest
@testable import VaultdayCore

/// A pending prefix that never times out (feedback item 23), and the keys
/// coming back to a pane the moment an input lets them go (item 29).
@MainActor
final class PendingStrokeTests: XCTestCase {
	private func makeSession() throws -> KeyboardSession {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		let focus = PaneFocusModel(scheme: .vim)
		let session = KeyboardSession(dispatcher: KeyDispatcher(keymap: keymap, scheme: .vim), focus: focus, register: PlanRegister(plan: FakePlan()) { _ in })
		focus.setItemCount(3, in: .today)
		focus.setItemCount(6, in: .sidebar)
		focus.setItemCount(2, in: .inbox)
		// On the Today pane itself, no card current, as `g2` leaves it.
		focus.focus(.today)
		return session
	}

	private func press(_ notation: String, in session: KeyboardSession) -> KeyResolution {
		session.handle(KeyStroke(notation: notation) ?? KeyStroke(key: "?"))
	}

	func testAnUnboundKeyAfterAPrefixLeavesNothingPending() throws {
		let session = try makeSession()
		XCTAssertEqual(press("g", in: session), .pending([KeyStroke(key: "g")]))
		XCTAssertTrue(session.dispatcher.showsPaneNumbers)
		XCTAssertEqual(press("q", in: session), .unbound)
		XCTAssertEqual(session.dispatcher.pendingText, "")
		XCTAssertTrue(session.dispatcher.completions.isEmpty)
		XCTAssertFalse(session.dispatcher.showsPaneNumbers, "the pane numbers leave with the prefix")
		XCTAssertEqual(session.focus.focusedPane, .today, "nothing ran")
	}

	func testAKeyThatBreaksAPrefixRunsOnItsOwn() throws {
		let session = try makeSession()
		_ = press("g", in: session)
		XCTAssertEqual(press("j", in: session), .action(.moveDown))
		XCTAssertEqual(session.focus.level, .row(.today, index: 0))
		XCTAssertEqual(session.dispatcher.pendingText, "")
	}

	func testEscWithAPrefixPendingOnlyDropsThePrefix() throws {
		let session = try makeSession()
		_ = press("j", in: session)
		_ = press("g", in: session)
		XCTAssertEqual(press("escape", in: session), .cancelled)
		XCTAssertEqual(session.focus.level, .row(.today, index: 0), "Esc did not also move a level up")
	}

	func testAPrefixWaitsAcrossAPointerFocusChange() throws {
		let session = try makeSession()
		_ = press("g", in: session)
		session.focus.select(1, in: .inbox)
		XCTAssertEqual(session.dispatcher.pendingText, "g", "no clock and no click drops it")
		XCTAssertEqual(press("1", in: session), .action(.focusPane(.sidebar)))
		XCTAssertEqual(session.focus.focusedPane, .sidebar)
	}

	func testMotionsActOnTheNextStrokeAfterAnInputGivesTheKeysBack() throws {
		let session = try makeSession()
		_ = press("j", in: session)
		session.enterInput(FocusedInput(name: "quick-add", pane: .today))
		XCTAssertEqual(session.focus.level, .input(FocusedInput(name: "quick-add", pane: .today)))
		// The field's Esc in Normal mode: AppKit's first responder leaves it, the app hears it by name.
		session.focus.leaveInput(named: "quick-add")
		XCTAssertEqual(session.focus.level, .row(.today, index: 0), "back at the level the pane had")
		XCTAssertEqual(press("j", in: session), .action(.moveDown))
		XCTAssertEqual(session.focus.level, .row(.today, index: 1))
		XCTAssertEqual(press("l", in: session), .action(.moveRight))
		XCTAssertEqual(session.focus.focusedPane, .inbox)
	}
}
