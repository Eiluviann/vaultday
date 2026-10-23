import Foundation
import XCTest
@testable import VaultdayCore

/// A Home window without views: the session as the app wires it, over a fake plan.
@MainActor
private final class Harness {
	let plan = FakePlan()
	let session: KeyboardSession
	private(set) var pasteboard: [String] = []
	/// The actions the session handed to the app.
	private(set) var appActions: [KeyAction] = []

	init(scheme: KeyScheme) throws {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		let dispatcher = KeyDispatcher(keymap: keymap, scheme: scheme)
		let focus = PaneFocusModel(scheme: scheme)
		session = KeyboardSession(dispatcher: dispatcher, focus: focus, register: PlanRegister(plan: plan) { _ in })
		session.selectedTaskId = { [unowned self] in selectedTaskId }
		session.onAction = { [unowned self] in appActions.append($0) }
		plan.add("task-1", "Write the solution design", scheduled: "2026-10-23")
		plan.add("task-2", "Chase the field mapping", scheduled: "2026-10-23")
		plan.add("task-3", "Update the catalogue mapping", scheduled: "2026-10-23")
		plan.add("task-9", "Archive the old widget", scheduled: nil)
		syncCount()
	}

	var selectedTaskId: String? {
		guard let index = session.focus.selection(in: .today) else { return nil }
		let list = plan.shownList
		return list.indices.contains(index) ? list[index] : nil
	}

	/// Types strokes in Keymap.yaml's notation, separated by spaces.
	func type(_ strokes: String) {
		for notation in strokes.split(separator: " ") {
			guard let stroke = KeyStroke(notation: String(notation)) else {
				XCTFail("not a stroke: \(notation)")
				continue
			}
			session.handle(stroke)
			syncCount()
		}
	}

	private func syncCount() {
		session.focus.setItemCount(plan.shownList.count, in: .today)
	}
}

@MainActor
final class KeyboardModelTests: XCTestCase {
	private func stroke(_ notation: String) -> KeyStroke {
		KeyStroke(notation: notation) ?? KeyStroke(key: "?")
	}

	private func repoKeymap() throws -> Keymap {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		return try Keymap.parse(text).get()
	}

	// MARK: The key table

	func testTheRepositoryKeymapNamesAndLabelsEveryAction() throws {
		let keymap = try repoKeymap()
		for scheme in KeyScheme.allCases {
			let table = keymap.table(for: scheme)
			XCTAssertFalse(table.isEmpty)
			for row in table {
				XCTAssertFalse(row.label.isEmpty, "\(row.action) has no label")
				XCTAssertFalse(row.label.hasSuffix("."), "a single-sentence label ends without a period: \(row.label)")
			}
			for (sequence, action, _) in keymap.sequences(for: scheme) {
				guard let first = sequence.first, first.isBare, let kept = Keymap.directionalKeys[first.key] else { continue }
				XCTAssertEqual(sequence.count, 1, "bare \(first.key) starts a chord under \(scheme)")
				XCTAssertEqual(action, kept, "bare \(first.key) is bound to \(action) under \(scheme)")
			}
		}
		// Vim motions are never actions; `n` `N` `{` `}` keep their vim meaning in Browse.
		for motion in ["w", "b", "e", "f", "t", "0", "$"] {
			XCTAssertFalse(keymap.sequences(for: .vim).contains { $0.sequence.first == stroke(motion) }, "the vim motion \(motion) starts a binding")
		}
		let vim = KeyDispatcher(keymap: keymap, scheme: .vim)
		let expected: [(String, KeyAction)] = [
			("g d", .showToday), ("alt+h", .previousDay), ("alt+l", .nextDay),
			("y y", .yank), ("d d", .cut), ("p", .pasteAfter), ("shift+p", .pasteBefore), ("u", .undo),
			("ctrl+r", .redo), (".", .repeatLast), ("x", .completeTask), ("c c", .changeText), ("c e", .changeEstimate),
			("c d", .changeDue), ("c s", .changeScheduled), ("< <", .pullToToday), ("m", .togglePin),
			("=", .autoSchedule), ("r", .replan), ("g t", .nextWorkspace), ("g shift+t", .previousWorkspace), ("g b", .showBrowse),
			("g l", .showDayLog), ("cmd+l", .appendDayLog), ("space", .toggleTimer),
			("alt+j", .moveTaskDown), ("alt+k", .moveTaskUp), ("g 0", .focusPane(.sidebar)), ("g 3", .focusPane(.sidePanel)),
			("]", .nextSidePanelTab), ("[", .previousSidePanelTab), ("alt+1", .showSidePanelTab(1)), ("alt+7", .showSidePanelTab(7)),
			("i", .addTask), ("o", .addTaskAfter), ("shift+o", .addTaskBefore), (":", .openPalette), ("cmd+k", .openPalette),
		]
		for (notation, action) in expected {
			var result: KeyResolution = .unbound
			for part in notation.split(separator: " ") { result = vim.handle(stroke(String(part))) }
			XCTAssertEqual(result, .action(action), notation)
		}
		// `>>` is "later" for the selection: a task in Today, a signal in the Inbox.
		for (pane, action) in [(Pane.today, KeyAction.pushToTomorrow), (.inbox, .signalSnooze)] {
			XCTAssertEqual(vim.handle(stroke(">"), in: pane), .pending([stroke(">")]))
			XCTAssertEqual(vim.handle(stroke(">"), in: pane), .action(action), "> > in \(pane)")
		}
		XCTAssertEqual(vim.handle(stroke("cmd+shift+c")), .action(.openInClaudeCode), "⌘⇧C is Claude Code, as the preview's button")
		XCTAssertEqual(vim.handle(stroke("g")), .pending([stroke("g")]))
		XCTAssertEqual(vim.handle(stroke("x")), .action(.openLink), "gx opens the selected item's link")
		XCTAssertFalse(vim.opensItemMenu, "a settled gx leaves the item menu shut")
	}

	func testTheValidatorRefusesTheDesignNotesBrokenRules() {
		let header = "version: 1\nbindings:\n"
		let cases: [(name: String, text: String, line: Int)] = [
			("bare h bound to an action", header + "  - action: open\n    label: \"Open\"\n    vim: \"h\"\n", 3),
			("bare l starting a chord", header + "  - action: open\n    label: \"Open\"\n    vim: \"l x\"\n", 3),
			("bare k under Standard", header + "  - action: task.done\n    label: \"Done\"\n    standard: \"k\"\n", 3),
			("a duplicate chord", header + "  - action: open\n    label: \"Open\"\n    vim: \"g x\"\n  - action: peek\n    label: \"Peek\"\n    vim: \"g x\"\n", 6),
			("a chord hidden by its prefix", header + "  - action: open\n    label: \"Open\"\n    vim: \"g\"\n  - action: peek\n    label: \"Peek\"\n    vim: \"g x\"\n", 3),
			("an action without a label", header + "  - action: open\n    vim: \"x\"\n", 3),
			("an unknown field", header + "  - action: open\n    lable: \"Open\"\n", 4),
			("an unknown pane", header + "  - action: open\n    label: \"Open\"\n    pane: \"dock\"\n    vim: \"x\"\n", 3),
			("a pane key also bound everywhere", header + "  - action: open\n    label: \"Open\"\n    vim: \"1\"\n  - action: peek\n    label: \"Peek\"\n    pane: \"inbox\"\n    vim: \"1\"\n", 6),
		]
		for entry in cases {
			switch Keymap.parse(entry.text) {
			case .success: XCTFail("accepted \(entry.name)")
			case let .failure(error): XCTAssertEqual(error.line, entry.line, "\(entry.name): \(error)")
			}
		}
		// Directional movement itself, a capital H and ⌥h stay allowed.
		let fine = header + "  - action: move.left\n    label: \"Left\"\n    vim: \"h\"\n  - action: day.previous\n    label: \"Previous day\"\n    vim: \"alt+h\"\n  - action: peek\n    label: \"Peek\"\n    vim: \"shift+h\"\n"
		XCTAssertNoThrow(try Keymap.parse(fine).get())
		// One key on two panes is no clash: only one of them holds focus.
		let twoPanes = header + "  - action: open\n    label: \"Open\"\n    pane: \"side-panel\"\n    vim: \"1\"\n  - action: peek\n    label: \"Peek\"\n    pane: \"inbox\"\n    vim: \"1\"\n"
		XCTAssertNoThrow(try Keymap.parse(twoPanes).get())
	}

	func testBareDigitsReachTheRailOnlyWhileTheSidePanelHoldsFocus() throws {
		let vim = KeyDispatcher(keymap: try repoKeymap(), scheme: .vim)
		XCTAssertEqual(vim.handle(stroke("1"), in: .sidePanel), .action(.showSidePanelTab(1)))
		XCTAssertEqual(vim.handle(stroke("7"), in: .sidePanel), .action(.showSidePanelTab(7)))
		for pane in [Pane.sidebar, .today] {
			XCTAssertEqual(vim.handle(stroke("1"), in: pane), .unbound, "bare 1 is free outside the side panel and the Inbox (\(pane))")
		}
		// The Inbox keeps its digits for the signal outcomes.
		XCTAssertEqual(vim.handle(stroke("1"), in: .inbox), .action(.signalTask))
		XCTAssertEqual(vim.handle(stroke("4"), in: .inbox), .action(.signalDismiss))
		XCTAssertEqual(vim.handle(stroke("5"), in: .inbox), .unbound)
		XCTAssertEqual(vim.handle(stroke("alt+2"), in: .inbox), .action(.showSidePanelTab(2)))
		// `g` then a digit is still a pane jump, wherever focus is.
		XCTAssertEqual(vim.handle(stroke("g"), in: .sidePanel), .pending([stroke("g")]))
		XCTAssertEqual(vim.handle(stroke("1"), in: .sidePanel), .action(.focusPane(.today)))
	}

	func testShiftedSymbolsMatchWithOrWithoutShiftInTheFile() {
		XCTAssertEqual(KeyStroke(notation: ">"), KeyStroke(notation: "shift+>"))
		XCTAssertEqual(KeyStroke(key: ":", modifiers: .shift), KeyStroke(notation: ":"))
		XCTAssertEqual(KeyStroke(notation: ">")?.capText, ">")
		XCTAssertEqual(KeyStroke(notation: "shift+g")?.capText, "G")
		XCTAssertEqual(KeyStroke(notation: "ctrl+r")?.capText, "⌃R")
		XCTAssertEqual(KeyStroke(notation: "shift+cmd+z")?.capText, "⇧⌘Z")
		XCTAssertEqual(KeyStroke(notation: "\\")?.capText, "\\")
	}

	// MARK: Pending strokes

	func testAPendingStrokeListsItsCompletionsAndTimesOut() throws {
		var now = ContinuousClock.now
		let dispatcher = KeyDispatcher(keymap: try repoKeymap(), scheme: .vim, pendingTimeout: .milliseconds(800), now: { now })

		XCTAssertEqual(dispatcher.handle(stroke("g")), .pending([stroke("g")]))
		XCTAssertEqual(dispatcher.pendingText, "g")
		XCTAssertTrue(dispatcher.showsPaneNumbers)
		XCTAssertFalse(dispatcher.opensItemMenu)
		let gNext = Set(dispatcher.completions.map(\.next.capText))
		XCTAssertTrue(gNext.isSuperset(of: ["0", "1", "2", "3", "g", "b", "l", "t", "T"]), "\(gNext)")

		now += .milliseconds(799)
		XCTAssertFalse(dispatcher.expirePending())
		XCTAssertEqual(dispatcher.handle(stroke("1")), .action(.focusPane(.today)), "inside the timeout the chord completes")

		_ = dispatcher.handle(stroke("g"))
		now += .milliseconds(800)
		XCTAssertTrue(dispatcher.expirePending())
		XCTAssertEqual(dispatcher.pendingText, "")
		XCTAssertTrue(dispatcher.completions.isEmpty)

		_ = dispatcher.handle(stroke("g"))
		now += .seconds(2)
		XCTAssertEqual(dispatcher.handle(stroke("1"), in: .sidePanel), .action(.showSidePanelTab(1)), "a lapsed g leaves 1 to stand alone")

		XCTAssertEqual(dispatcher.handle(stroke("y")), .pending([stroke("y")]))
		XCTAssertTrue(dispatcher.opensItemMenu, "yy lives in the item's menu")
		XCTAssertFalse(dispatcher.showsPaneNumbers)
		XCTAssertEqual(dispatcher.handle(stroke("escape")), .action(.dismiss), "Esc breaks the pending y")
		XCTAssertEqual(dispatcher.pending, [])

		XCTAssertEqual(dispatcher.handle(stroke("c")), .pending([stroke("c")]))
		XCTAssertEqual(dispatcher.completions.map(\.next.key), ["c", "e", "d", "s"])
		XCTAssertEqual(dispatcher.completions.map(\.label), ["Change the text", "Change the estimate", "Change the due day", "Change the scheduled day"])
	}

	func testSwitchingSchemesDropsThePendingStrokeAndTheFocusRule() throws {
		let harness = try Harness(scheme: .vim)
		harness.type("g")
		XCTAssertEqual(harness.session.dispatcher.pendingText, "g")
		harness.session.setScheme(.standard)
		XCTAssertEqual(harness.session.dispatcher.pendingText, "")
		XCTAssertEqual(harness.session.dispatcher.scheme, .standard)
		XCTAssertEqual(harness.session.focus.scheme, .standard)
		harness.type("j")
		XCTAssertEqual(harness.selectedTaskId, "task-1", "a bare j is not a Standard key")
		harness.type("ctrl+2")
		XCTAssertEqual(harness.session.focus.focusedPane, .inbox)
		harness.type("ctrl+1")
		XCTAssertEqual(harness.selectedTaskId, "task-1", "Standard lands on the first item")
		harness.session.setScheme(.vim)
		harness.type("g 1")
		XCTAssertTrue(harness.session.focus.isOnContainer(.today), "Vim motions focus the container")
		XCTAssertNil(harness.selectedTaskId)
	}

	func testHoverSelectsOnlyWhenTheSettingIsOn() {
		let focus = PaneFocusModel()
		focus.setItemCount(4, in: .today)
		XCTAssertTrue(focus.hover(2, in: .today))
		XCTAssertEqual(focus.selection(in: .today), 2)
		focus.hoverSelects = false
		XCTAssertFalse(focus.hover(3, in: .today))
		XCTAssertEqual(focus.selection(in: .today), 2)
	}

	func testHalfPageMovesByHalfTheVisibleRows() {
		let focus = PaneFocusModel()
		focus.setItemCount(30, in: .today)
		focus.setVisibleRowCount(12, in: .today)
		focus.apply(.moveHalfPageDown)
		XCTAssertEqual(focus.selection(in: .today), 6)
		focus.apply(.moveHalfPageUp)
		focus.apply(.moveHalfPageUp)
		XCTAssertEqual(focus.selection(in: .today), 0)
	}

	// MARK: Storyboards

	/// The Vim storyboard, six steps: pending g with pane numbers, g1 onto
	/// the Today container, j j to a task, dd to the backlog with the menu
	/// opening on the pending d, P pastes it back before the selection, and
	/// u then ^r undo and redo the paste.
	func testTheVimStoryboard() throws {
		let harness = try Harness(scheme: .vim)
		let session = harness.session
		harness.session.focus.focus(.inbox)

		// 1. g: the showcmd and the pane numbers.
		harness.type("g")
		XCTAssertEqual(session.dispatcher.pendingText, "g")
		XCTAssertTrue(session.dispatcher.showsPaneNumbers)
		XCTAssertFalse(session.showsItemMenu)

		// 2. 1: Today, the container itself.
		harness.type("1")
		XCTAssertEqual(session.focus.focusedPane, .today)
		XCTAssertTrue(session.focus.isOnContainer(.today))
		XCTAssertNil(harness.selectedTaskId)
		XCTAssertEqual(session.dispatcher.pendingText, "")

		// 3. j j: the first item, then the second.
		harness.type("j")
		XCTAssertEqual(harness.selectedTaskId, "task-1")
		harness.type("j")
		XCTAssertEqual(harness.selectedTaskId, "task-2")

		// 4. d d: the menu opens on d with dd lit, then task-2 leaves for the backlog.
		harness.type("d")
		XCTAssertTrue(session.showsItemMenu)
		XCTAssertEqual(session.dispatcher.completions.filter(\.isInItemMenu).map(\.action), [.cut])
		harness.type("d")
		XCTAssertFalse(session.showsItemMenu)
		XCTAssertNil(harness.plan.lines["task-2"]?.scheduled)
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-3"])
		XCTAssertEqual(session.register.item?.taskId, "task-2")
		XCTAssertEqual(harness.selectedTaskId, "task-3", "the selection stays at its place")

		// 5. P: pasted before the selection, scheduled on the day shown.
		harness.type("shift+p")
		XCTAssertEqual(harness.plan.lines["task-2"]?.scheduled, "2026-10-23")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-2", "task-3"])

		// 6. u then ^r: the paste undone, then made again.
		harness.type("u")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-3"])
		XCTAssertNil(harness.plan.lines["task-2"]?.scheduled)
		harness.type("ctrl+r")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-2", "task-3"])
		XCTAssertEqual(session.register.undoStack.count, 2)
		XCTAssertTrue(harness.appActions.allSatisfy { !KeyboardSession.repeatableActions.contains($0) && ![.undo, .redo].contains($0) }, "the register ran every change")
	}

	/// The Standard storyboard, six steps: ⌃1 lands on the first item, ↓
	/// moves, ⌘X cuts to the backlog, ↑ and ⌘V paste after the selection,
	/// ⌘Z undoes it, ⇧⌘Z redoes it and ⌘K opens the palette. No stroke is
	/// ever pending.
	func testTheStandardStoryboard() throws {
		let harness = try Harness(scheme: .standard)
		let session = harness.session
		session.focus.focus(.sidebar)

		// 1. ⌃1: Today, on its first item.
		harness.type("ctrl+1")
		XCTAssertEqual(session.focus.focusedPane, .today)
		XCTAssertFalse(session.focus.isOnContainer(.today))
		XCTAssertEqual(harness.selectedTaskId, "task-1")

		// 2. ↓: the second item.
		harness.type("down")
		XCTAssertEqual(harness.selectedTaskId, "task-2")

		// 3. ⌘X: cut to the backlog.
		harness.type("cmd+x")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-3"])
		XCTAssertEqual(session.register.item?.taskId, "task-2")

		// 4. ↑ ⌘V: pasted after the first item.
		harness.type("up cmd+v")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-2", "task-3"])
		XCTAssertEqual(harness.plan.lines["task-2"]?.scheduled, "2026-10-23")

		// 5. ⌘Z: the paste undone.
		harness.type("cmd+z")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-3"])

		// 6. ⇧⌘Z and ⌘K: redone, then the palette.
		harness.type("shift+cmd+z cmd+k")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-2", "task-3"])
		XCTAssertEqual(harness.appActions.last, .openPalette)
		XCTAssertFalse(harness.appActions.contains(.cut), "the register ran the cut")
		XCTAssertEqual(session.dispatcher.pendingText, "", "Standard keys never pend")
	}

	// MARK: Register, undo and repeat

	func testDoneUndoRedoAndRepeatGoThroughTheRegister() throws {
		let harness = try Harness(scheme: .vim)
		harness.type("g 1 j x")
		XCTAssertEqual(harness.plan.lines["task-1"]?.completion, "2026-10-23")
		harness.type("u")
		XCTAssertNil(harness.plan.lines["task-1"]?.completion)
		harness.type("ctrl+r")
		XCTAssertNotNil(harness.plan.lines["task-1"]?.completion)

		// . repeats the last change on the new selection.
		harness.type("j > >")
		XCTAssertEqual(harness.plan.lines["task-2"]?.scheduled, "2026-10-24")
		harness.type(".")
		XCTAssertEqual(harness.plan.lines["task-3"]?.scheduled, "2026-10-24", "the list moved up under the selection")
		harness.type("u u")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-2", "task-3"])
	}

	func testARefusedEditKeepsHistoryAndSaysWhy() throws {
		let harness = try Harness(scheme: .vim)
		harness.type("g 1 j")
		harness.plan.refusing = ["task-1"]
		harness.type("d d")
		XCTAssertEqual(harness.plan.shownList, ["task-1", "task-2", "task-3"])
		XCTAssertTrue(harness.session.register.undoStack.isEmpty)
		XCTAssertEqual(harness.session.register.message, "the line changed while it was written")
		harness.type("p")
		XCTAssertEqual(harness.session.register.message, "the line changed while it was written", "the register held task-1 from dd")
		harness.type("u")
		XCTAssertEqual(harness.session.register.message, "Nothing to undo")
	}

	func testYankCopiesTheIdAndPasteMovesTheBacklogTaskIn() throws {
		var copied: [String] = []
		let plan = FakePlan()
		plan.add("task-1", "One", scheduled: "2026-10-23")
		plan.add("task-9", "Backlog", scheduled: nil)
		let register = PlanRegister(plan: plan) { copied.append($0) }
		register.yank("task-9")
		XCTAssertEqual(copied, ["task-9"])
		register.paste(after: true, anchor: "task-1")
		XCTAssertEqual(plan.shownList, ["task-1", "task-9"])
		register.paste(after: false, anchor: "task-1")
		XCTAssertEqual(plan.shownList, ["task-9", "task-1"], "a second paste moves it rather than doubling it")
		register.undo()
		register.undo()
		XCTAssertEqual(plan.shownList, ["task-1"])
		XCTAssertFalse(register.canUndo)
		XCTAssertTrue(register.canRedo)
		register.complete("task-1")
		XCTAssertFalse(register.canRedo, "a new change clears redo")
	}

	func testUndoRefusesALineThatChangedSinceAndKeepsTheChange() {
		let plan = FakePlan()
		plan.add("task-1", "One", scheduled: "2026-10-23")
		let register = PlanRegister(plan: plan) { _ in }
		register.pushToTomorrow("task-1")
		plan.lines["task-1"]?.text = "Edited in the note meanwhile"
		register.undo()
		XCTAssertEqual(register.message, "the line changed since")
		XCTAssertEqual(register.undoStack.count, 1, "the change stays for a later undo")
		XCTAssertEqual(plan.lines["task-1"]?.scheduled, "2026-10-24")
	}

	func testChangeKeysOpenAnEditorAndSaveThroughTheRegister() throws {
		let harness = try Harness(scheme: .vim)
		harness.type("g 1 j c e")
		XCTAssertEqual(harness.session.changeRequest?.field, .estimate)
		XCTAssertEqual(harness.session.changeRequest?.title, "Change the estimate")
		harness.session.submitChange("45m")
		XCTAssertEqual(harness.plan.lines["task-1"]?.estimate, "45m")
		harness.type("c s")
		harness.session.submitChange("tomorrow")
		XCTAssertEqual(harness.plan.lines["task-1"]?.scheduled, "2026-10-24")
		harness.type("u u")
		XCTAssertEqual(harness.plan.lines["task-1"], TaskLineState(text: "Write the solution design", scheduled: "2026-10-23"))
		harness.type("c d")
		harness.session.submitChange("someday")
		XCTAssertEqual(harness.session.register.message, "Not a day: someday")
		harness.type("c c")
		harness.session.submitChange("  ")
		XCTAssertEqual(harness.session.register.message, "A task needs text")
	}

	func testTheItemMenuKeyTogglesAndEscCloses() throws {
		let harness = try Harness(scheme: .vim)
		harness.type("g 1 j \\")
		XCTAssertTrue(harness.session.showsItemMenu)
		harness.type("escape")
		XCTAssertFalse(harness.session.showsItemMenu)
		XCTAssertFalse(harness.appActions.contains(.dismiss), "Esc closed the menu, nothing else")
	}

	// MARK: Palette and footer

	func testThePaletteRendersTheKeyTableUnderBothSchemes() throws {
		let keymap = try repoKeymap()
		for scheme in KeyScheme.allCases {
			let dispatcher = KeyDispatcher(keymap: keymap, scheme: scheme)
			let palette = CommandPaletteModel(dispatcher: dispatcher)
			let commands = palette.sections.filter { ![CommandPaletteModel.recentStoriesTitle, CommandPaletteModel.workspacesTitle].contains($0.title) }
			let rendered = commands.flatMap(\.rows).map { "\($0.title) | \($0.caps)" }
			let table = keymap.table(for: scheme).map { "\($0.label) | \($0.caps)" }
			XCTAssertEqual(rendered.sorted(), table.sorted(), "\(scheme)")
			XCTAssertEqual(Set(commands.map(\.title)), Set(keymap.table(for: scheme).map(\.group)))
		}
		let vim = CommandPaletteModel(dispatcher: KeyDispatcher(keymap: keymap, scheme: .vim))
		XCTAssertEqual(vim.rows.first { $0.choice == .action(.cut) }?.caps, [["d", "d"]])
		XCTAssertEqual(vim.rows.first { $0.choice == .action(.openPalette) }?.caps, [["⌘K"], [":"]])
		XCTAssertEqual(vim.rows.first { $0.choice == .action(.togglePin) }?.caps, [["m"], ["X"]])
	}

	func testThePalettesEmptyStateAndWorkspaceJump() throws {
		let palette = CommandPaletteModel(dispatcher: KeyDispatcher(keymap: try repoKeymap(), scheme: .vim))
		var recent = RecentStories()
		recent.touch(PaletteStory(number: "WEB-142", title: "Checkout redesign"))
		recent.touch(PaletteStory(number: "OPS-12", title: "Runbook refresh"))
		recent.touch(PaletteStory(number: "WEB-142", title: "Checkout redesign"))
		palette.recentStories = recent.filled(with: [PaletteStory(number: "OPS-12", title: "Runbook refresh"), PaletteStory(number: "WEB-150", title: "Cart totals")])
		palette.workspaces = [PaletteWorkspace(id: "w1", name: "Work"), PaletteWorkspace(id: "w2", name: "Personal")]
		palette.open()

		XCTAssertEqual(palette.sections.first?.title, CommandPaletteModel.recentStoriesTitle)
		XCTAssertEqual(palette.sections.first?.rows.map(\.detail), ["WEB-142", "OPS-12", "WEB-150"])
		XCTAssertEqual(palette.sections.dropFirst().first?.title, CommandPaletteModel.workspacesTitle)

		palette.query = "personal"
		XCTAssertEqual(palette.rows.map(\.choice), [.workspace("w2")])
		XCTAssertEqual(palette.choose(), .workspace("w2"))
		XCTAssertFalse(palette.isOpen)

		palette.open()
		palette.query = "cut back"
		XCTAssertEqual(palette.rows.map(\.choice), [.action(.cut)])
		palette.query = "web-150"
		XCTAssertEqual(palette.choose(), .story("WEB-150"))

		palette.open()
		palette.moveSelection(by: -3)
		XCTAssertEqual(palette.selectedIndex, 0)
		palette.moveSelection(by: 10_000)
		XCTAssertEqual(palette.selectedIndex, palette.rows.count - 1)
	}

	func testTheFooterTipsComeFromTheKeymap() throws {
		let keymap = try repoKeymap()
		let vim = KeyDispatcher(keymap: keymap, scheme: .vim)
		XCTAssertEqual(vim.tips.map(\.name), ["timer", "done", "lock", "add", "palette", "re-plan"])
		XCTAssertEqual(vim.tips.compactMap { vim.label(for: $0.action) }, ["Space", "x", "m", "i", "⌘K", "r"])
		let standard = KeyDispatcher(keymap: keymap, scheme: .standard)
		XCTAssertEqual(standard.tips.compactMap { standard.label(for: $0.action) }, ["Space", "⌘D", "⌥⌘L", "⌘N", "⌘K", "⌘R"])
	}

	func testShortcutCapsMotionFollowsTheSettingAndReduceMotion() {
		XCTAssertEqual(ShortcutCapMotion(setting: .animated, reduceMotion: false), .grow(duration: 0.16))
		XCTAssertEqual(ShortcutCapMotion(setting: .animated, reduceMotion: true), .crossfade(duration: 0.15))
		XCTAssertEqual(ShortcutCapMotion(setting: .instant, reduceMotion: false), .instant)
		XCTAssertEqual(ShortcutCapMotion(setting: .instant, reduceMotion: true), .instant)
	}
}
