import XCTest
@testable import VaultdayCore

/// The Vim field's editing model, key by key, as the text view drives it:
/// Insert mode's characters are typed by the "text view" here, the way
/// AppKit types them, and reported back through `syncInsert`.
final class VimEditorTests: XCTestCase {
	private struct Case {
		let text: String
		let cursor: Int
		let keys: String
		let expectedText: String
		let expectedCursor: Int
		var expectedMode: VimMode = .normal
		var isMultiline = false
		let line: UInt
	}

	/// Keys in Vim's spelling: `<Esc>`, `<CR>`, `<C-r>`, `<BS>`, anything else one character each.
	private static func keys(_ notation: String) -> [VimKey] {
		var keys: [VimKey] = []
		var rest = Substring(notation)
		let named: [(String, VimKey)] = [("<Esc>", .escape), ("<CR>", .enter), ("<C-r>", .control("r")), ("<BS>", .backspace)]
		while let first = rest.first {
			if let (name, key) = named.first(where: { rest.hasPrefix($0.0) }) {
				keys.append(key)
				rest = rest.dropFirst(name.count)
			} else {
				keys.append(.character(first))
				rest = rest.dropFirst()
			}
		}
		return keys
	}

	/// Feeds keys; a key Insert mode passes through is typed at the caret.
	@discardableResult
	private static func type(_ notation: String, into editor: inout VimEditor) -> [VimEffect] {
		keys(notation).map { key in
			let effect = editor.handle(key)
			if effect == .passThrough, case let .character(character) = key {
				var characters = editor.characters
				characters.insert(character, at: editor.cursor)
				editor.syncInsert(text: String(characters), cursor: editor.cursor + 1)
			}
			return effect
		}
	}

	private static func editor(_ text: String, cursor: Int, isMultiline: Bool = false) -> VimEditor {
		var editor = VimEditor(text: text, isMultiline: isMultiline)
		editor.place(cursor: cursor)
		return editor
	}

	private func check(_ cases: [Case]) {
		for test in cases {
			var editor = Self.editor(test.text, cursor: test.cursor, isMultiline: test.isMultiline)
			Self.type(test.keys, into: &editor)
			XCTAssertEqual(editor.text, test.expectedText, "text after \(test.keys)", line: test.line)
			XCTAssertEqual(editor.cursor, test.expectedCursor, "cursor after \(test.keys)", line: test.line)
			XCTAssertEqual(editor.mode, test.expectedMode, "mode after \(test.keys)", line: test.line)
		}
	}

	func testAFieldStartsInNormalModeWithABlockCursor() {
		let editor = VimEditor(text: "abc")
		XCTAssertEqual(editor.mode, .normal)
		XCTAssertEqual(editor.cursor, 0)
		XCTAssertEqual(editor.selection, 0 ..< 1, "the block covers the character under the cursor")
		let inserting = VimEditor(text: "abc", mode: .insert)
		XCTAssertEqual(inserting.cursor, 3)
		XCTAssertEqual(inserting.selection, 3 ..< 3, "Insert mode shows a bar")
	}

	func testMotions() {
		check([
			Case(text: "hello world", cursor: 0, keys: "w", expectedText: "hello world", expectedCursor: 6, line: #line),
			Case(text: "hello world", cursor: 6, keys: "b", expectedText: "hello world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "e", expectedText: "hello world", expectedCursor: 4, line: #line),
			Case(text: "hello world", cursor: 0, keys: "ee", expectedText: "hello world", expectedCursor: 10, line: #line),
			Case(text: "hello world", cursor: 3, keys: "0", expectedText: "hello world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 3, keys: "$", expectedText: "hello world", expectedCursor: 10, line: #line),
			Case(text: "  hi", cursor: 3, keys: "^", expectedText: "  hi", expectedCursor: 2, line: #line),
			Case(text: "hello", cursor: 0, keys: "lll", expectedText: "hello", expectedCursor: 3, line: #line),
			Case(text: "hello", cursor: 4, keys: "l", expectedText: "hello", expectedCursor: 4, line: #line),
			Case(text: "hello", cursor: 4, keys: "3h", expectedText: "hello", expectedCursor: 1, line: #line),
			Case(text: "a b c d", cursor: 0, keys: "2w", expectedText: "a b c d", expectedCursor: 4, line: #line),
			Case(text: "foo.bar baz", cursor: 0, keys: "w", expectedText: "foo.bar baz", expectedCursor: 3, line: #line),
			Case(text: "hello world", cursor: 0, keys: "w", expectedText: "hello world", expectedCursor: 6, line: #line),
			Case(text: "one", cursor: 0, keys: "j", expectedText: "one", expectedCursor: 0, line: #line),
		])
	}

	func testOperatorsAndShorthands() {
		check([
			Case(text: "hello world", cursor: 0, keys: "x", expectedText: "ello world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "3x", expectedText: "lo world", expectedCursor: 0, line: #line),
			Case(text: "hello", cursor: 4, keys: "x", expectedText: "hell", expectedCursor: 3, line: #line),
			Case(text: "hello", cursor: 2, keys: "X", expectedText: "hllo", expectedCursor: 1, line: #line),
			Case(text: "hello world", cursor: 0, keys: "dw", expectedText: "world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 6, keys: "dw", expectedText: "hello ", expectedCursor: 5, line: #line),
			Case(text: "a b c d", cursor: 0, keys: "d2w", expectedText: "c d", expectedCursor: 0, line: #line),
			Case(text: "a b c d", cursor: 0, keys: "2dw", expectedText: "c d", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 4, keys: "db", expectedText: "o world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "de", expectedText: " world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 6, keys: "d0", expectedText: "world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 6, keys: "d$", expectedText: "hello ", expectedCursor: 5, line: #line),
			Case(text: "hello world", cursor: 6, keys: "D", expectedText: "hello ", expectedCursor: 5, line: #line),
			Case(text: "hello world", cursor: 3, keys: "dd", expectedText: "", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "cwhey<Esc>", expectedText: "hey world", expectedCursor: 2, line: #line),
			Case(text: "hello world", cursor: 0, keys: "ccnew<Esc>", expectedText: "new", expectedCursor: 2, line: #line),
			Case(text: "hello world", cursor: 0, keys: "Snew<Esc>", expectedText: "new", expectedCursor: 2, line: #line),
			Case(text: "hello world", cursor: 6, keys: "Cthere<Esc>", expectedText: "hello there", expectedCursor: 10, line: #line),
			Case(text: "hello", cursor: 0, keys: "sj<Esc>", expectedText: "jello", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "cw", expectedText: " world", expectedCursor: 0, expectedMode: .insert, line: #line),
		])
	}

	func testInsertCommands() {
		check([
			Case(text: "abc", cursor: 1, keys: "i-<Esc>", expectedText: "a-bc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 1, keys: "a-<Esc>", expectedText: "ab-c", expectedCursor: 2, line: #line),
			Case(text: "  abc", cursor: 4, keys: "I-<Esc>", expectedText: "  -abc", expectedCursor: 2, line: #line),
			Case(text: "abc", cursor: 0, keys: "Ad<Esc>", expectedText: "abcd", expectedCursor: 3, line: #line),
			Case(text: "", cursor: 0, keys: "ahi<Esc>", expectedText: "hi", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 0, keys: "o", expectedText: "abc", expectedCursor: 0, line: #line),
			Case(text: "abc", cursor: 0, keys: "ix", expectedText: "xabc", expectedCursor: 1, expectedMode: .insert, line: #line),
		])
	}

	func testYankAndPut() {
		check([
			Case(text: "hello", cursor: 0, keys: "xp", expectedText: "ehllo", expectedCursor: 1, line: #line),
			Case(text: "hello world", cursor: 0, keys: "ywP", expectedText: "hello hello world", expectedCursor: 5, line: #line),
			Case(text: "hello world", cursor: 0, keys: "yw$p", expectedText: "hello worldhello ", expectedCursor: 16, line: #line),
			Case(text: "ab", cursor: 0, keys: "yy$p", expectedText: "abab", expectedCursor: 3, line: #line),
			Case(text: "ab", cursor: 0, keys: "x2p", expectedText: "baa", expectedCursor: 2, line: #line),
		])
	}

	func testUndoRedoAndRepeat() {
		check([
			Case(text: "hello world", cursor: 0, keys: "dwu", expectedText: "hello world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "dwu<C-r>", expectedText: "world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "cwhey<Esc>u", expectedText: "hello world", expectedCursor: 0, line: #line),
			Case(text: "a b c", cursor: 0, keys: "x..", expectedText: " c", expectedCursor: 0, line: #line),
			Case(text: "one two three", cursor: 0, keys: "dw.", expectedText: "three", expectedCursor: 0, line: #line),
			Case(text: "abcdef", cursor: 0, keys: "x3.", expectedText: "ef", expectedCursor: 0, line: #line),
			Case(text: "abc", cursor: 0, keys: "ix<Esc>.", expectedText: "xxabc", expectedCursor: 0, line: #line),
			Case(text: "one two", cursor: 0, keys: "cwXY<Esc>w.", expectedText: "XY XY", expectedCursor: 4, line: #line),
			Case(text: "abc", cursor: 0, keys: "xxuu", expectedText: "abc", expectedCursor: 0, line: #line),
		])
	}

	func testVisualMode() {
		check([
			Case(text: "hello world", cursor: 0, keys: "vex", expectedText: " world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "vlld", expectedText: "lo world", expectedCursor: 0, line: #line),
			Case(text: "hello world", cursor: 0, keys: "vey$p", expectedText: "hello worldhello", expectedCursor: 15, line: #line),
			Case(text: "hello world", cursor: 6, keys: "vechi<Esc>", expectedText: "hello hi", expectedCursor: 7, line: #line),
			Case(text: "hello", cursor: 0, keys: "vl<Esc>", expectedText: "hello", expectedCursor: 1, line: #line),
			Case(text: "hello", cursor: 0, keys: "v2l", expectedText: "hello", expectedCursor: 2, expectedMode: .visual, line: #line),
		])
		var editor = Self.editor("hello", cursor: 1)
		Self.type("vl", into: &editor)
		XCTAssertEqual(editor.selection, 1 ..< 3, "Visual selects anchor to cursor, both included")
		Self.type("o", into: &editor)
		XCTAssertEqual(editor.cursor, 1)
		XCTAssertEqual(editor.selection, 1 ..< 3)
	}

	func testSeveralLines() {
		check([
			Case(text: "one\ntwo\nthree", cursor: 1, keys: "j", expectedText: "one\ntwo\nthree", expectedCursor: 5, isMultiline: true, line: #line),
			Case(text: "one\ntwo\nthree", cursor: 1, keys: "jjk", expectedText: "one\ntwo\nthree", expectedCursor: 5, isMultiline: true, line: #line),
			Case(text: "one\ntwo\nthree", cursor: 5, keys: "dd", expectedText: "one\nthree", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo\nthree", cursor: 9, keys: "dd", expectedText: "one\ntwo", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo", cursor: 0, keys: "yyp", expectedText: "one\none\ntwo", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo", cursor: 4, keys: "yyP", expectedText: "one\ntwo\ntwo", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo", cursor: 0, keys: "ox<Esc>", expectedText: "one\nx\ntwo", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo", cursor: 4, keys: "Ox<Esc>", expectedText: "one\nx\ntwo", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo\nthree", cursor: 0, keys: "G", expectedText: "one\ntwo\nthree", expectedCursor: 8, isMultiline: true, line: #line),
			Case(text: "one\ntwo\nthree", cursor: 9, keys: "gg", expectedText: "one\ntwo\nthree", expectedCursor: 0, isMultiline: true, line: #line),
			Case(text: "one\ntwo\nthree", cursor: 0, keys: "dj", expectedText: "three", expectedCursor: 0, isMultiline: true, line: #line),
			Case(text: "one\ntwo", cursor: 2, keys: "$", expectedText: "one\ntwo", expectedCursor: 2, isMultiline: true, line: #line),
		])
	}

	func testEscLeavesFromNormalAndStepsDownFromInsert() {
		var editor = VimEditor(text: "abc")
		XCTAssertEqual(Self.type("d", into: &editor), [.handled])
		XCTAssertEqual(editor.pendingText, "d")
		XCTAssertEqual(Self.type("<Esc>", into: &editor), [.handled], "Esc first drops the pending operator")
		XCTAssertEqual(editor.pendingText, "")
		XCTAssertEqual(Self.type("<Esc>", into: &editor), [.leave], "then leaves the field")

		var inserting = VimEditor(text: "abc", mode: .insert)
		XCTAssertEqual(Self.type("<Esc>", into: &inserting), [.handled], "Esc in Insert goes to Normal first")
		XCTAssertEqual(inserting.mode, .normal)
		XCTAssertEqual(Self.type("<Esc>", into: &inserting), [.leave])
	}

	func testReturnSubmitsAOneLineField() {
		var editor = VimEditor(text: "abc")
		XCTAssertEqual(editor.handle(.enter), .submit)
		var inserting = VimEditor(text: "abc", mode: .insert)
		XCTAssertEqual(inserting.handle(.enter), .submit)
		XCTAssertEqual(inserting.mode, .insert, "the field keeps typing after a submit")
		var lines = VimEditor(text: "a\nb", isMultiline: true, mode: .insert)
		XCTAssertEqual(lines.handle(.enter), .passThrough, "⏎ types a newline in a multi-line field")
	}

	func testThePendingKeysShowUntilTheCommandCompletes() {
		var editor = VimEditor(text: "one two three")
		Self.type("2d", into: &editor)
		XCTAssertEqual(editor.pendingText, "2d")
		Self.type("w", into: &editor)
		XCTAssertEqual(editor.pendingText, "")
		XCTAssertEqual(editor.text, "three")
		Self.type("g", into: &editor)
		XCTAssertEqual(editor.pendingText, "g")
	}

	func testStartInsertingAppendsAndIsOneUndoStep() {
		var editor = VimEditor(text: "buy milk")
		editor.startInserting()
		XCTAssertEqual(editor.mode, .insert)
		XCTAssertEqual(editor.cursor, 8)
		Self.type(" now<Esc>u", into: &editor)
		XCTAssertEqual(editor.text, "buy milk")
	}
}
