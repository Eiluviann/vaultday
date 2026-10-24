import XCTest
@testable import VaultdayCore

/// The Vim field against Vim itself: every expected text and cursor here is
/// what Vim 9 does with the same keys on the same line (default options).
final class VimEditorVimParityTests: XCTestCase {
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

	private static func keys(_ notation: String) -> [VimKey] {
		var keys: [VimKey] = []
		var rest = Substring(notation)
		let named: [(String, VimKey)] = [("<Esc>", .escape), ("<CR>", .enter), ("<C-r>", .control("r")), ("<BS>", .backspace), ("<Left>", .left), ("<Right>", .right)]
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
			XCTAssertEqual(editor.text, test.expectedText, "text after \(test.keys) on \(test.text.debugDescription)", line: test.line)
			XCTAssertEqual(editor.cursor, test.expectedCursor, "cursor after \(test.keys) on \(test.text.debugDescription)", line: test.line)
			XCTAssertEqual(editor.mode, test.expectedMode, "mode after \(test.keys) on \(test.text.debugDescription)", line: test.line)
		}
	}

	func testCountsAndOperatorsWithMotions() {
		check([
			Case(text: "one two three four", cursor: 0, keys: "d2w", expectedText: "three four", expectedCursor: 0, line: #line),
			Case(text: "one two three four", cursor: 0, keys: "2d2w", expectedText: "", expectedCursor: 0, line: #line),
			Case(text: "one two three four", cursor: 4, keys: "d3w", expectedText: "one ", expectedCursor: 3, line: #line),
			Case(text: "hello world", cursor: 6, keys: "c$there<Esc>", expectedText: "hello there", expectedCursor: 10, line: #line),
			Case(text: "hello world", cursor: 6, keys: "c$", expectedText: "hello ", expectedCursor: 6, expectedMode: .insert, line: #line),
			Case(text: "abcdef", cursor: 1, keys: "3x", expectedText: "aef", expectedCursor: 1, line: #line),
			Case(text: "abcdef", cursor: 4, keys: "3x", expectedText: "abcd", expectedCursor: 3, line: #line),
			Case(text: "abcdefghijklmnop", cursor: 0, keys: "10l", expectedText: "abcdefghijklmnop", expectedCursor: 10, line: #line),
			Case(text: "abcdef", cursor: 5, keys: "2X", expectedText: "abcf", expectedCursor: 3, line: #line),
			Case(text: "foo bar", cursor: 4, keys: "db", expectedText: "bar", expectedCursor: 0, line: #line),
			Case(text: "foo bar", cursor: 0, keys: "de", expectedText: " bar", expectedCursor: 0, line: #line),
			Case(text: "a b c", cursor: 0, keys: "c2wX<Esc>", expectedText: "X c", expectedCursor: 0, line: #line),
			Case(text: "a  b", cursor: 1, keys: "cwX<Esc>", expectedText: "aXb", expectedCursor: 1, line: #line),
			Case(text: "hello", cursor: 3, keys: "d0", expectedText: "lo", expectedCursor: 0, line: #line),
			Case(text: "foo bar", cursor: 4, keys: "w", expectedText: "foo bar", expectedCursor: 6, line: #line),
			Case(text: "  abc", cursor: 4, keys: "^", expectedText: "  abc", expectedCursor: 2, line: #line),
			Case(text: "  abc", cursor: 4, keys: "0", expectedText: "  abc", expectedCursor: 0, line: #line),
		])
	}

	func testRepeatAfterChanges() {
		check([
			Case(text: "foo bar baz", cursor: 0, keys: "cwxx<Esc>w.", expectedText: "xx xx baz", expectedCursor: 4, line: #line),
			Case(text: "foo bar baz", cursor: 0, keys: "cwxx<Esc>w.w.", expectedText: "xx xx xx", expectedCursor: 7, line: #line),
			Case(text: "a b c d e", cursor: 0, keys: "dw2.", expectedText: "d e", expectedCursor: 0, line: #line),
			Case(text: "abcdefgh", cursor: 0, keys: "2x.", expectedText: "efgh", expectedCursor: 0, line: #line),
			Case(text: "abc", cursor: 0, keys: "A!<Esc>.", expectedText: "abc!!", expectedCursor: 4, line: #line),
			Case(text: "hello world", cursor: 6, keys: "D0.", expectedText: "", expectedCursor: 0, line: #line),
			Case(text: "ab", cursor: 0, keys: "ylp.", expectedText: "aaab", expectedCursor: 2, line: #line),
		])
	}

	func testUndoGranularity() {
		// One Insert session is one undo step, however much it typed.
		var editor = Self.editor("abc", cursor: 0)
		Self.type("Adef ghi<Esc>", into: &editor)
		XCTAssertEqual(editor.text, "abcdef ghi")
		Self.type("u", into: &editor)
		XCTAssertEqual(editor.text, "abc")
		Self.type("u", into: &editor)
		XCTAssertEqual(editor.text, "abc", "nothing older to undo")
		Self.type("<C-r>", into: &editor)
		XCTAssertEqual(editor.text, "abcdef ghi")
		// Two sessions are two steps.
		var twice = Self.editor("x", cursor: 0)
		Self.type("ia<Esc>ib<Esc>", into: &twice)
		XCTAssertEqual(twice.text, "bax")
		Self.type("u", into: &twice)
		XCTAssertEqual(twice.text, "ax")
		Self.type("u", into: &twice)
		XCTAssertEqual(twice.text, "x")
		// A `.` replay of a change is one step too.
		var replay = Self.editor("foo bar", cursor: 0)
		Self.type("cwX<Esc>w.u", into: &replay)
		XCTAssertEqual(replay.text, "X bar")
		// A new change drops what could be redone.
		var redo = Self.editor("abc", cursor: 0)
		Self.type("xux<C-r>", into: &redo)
		XCTAssertEqual(redo.text, "bc")
	}

	func testPutAfterYankAndDelete() {
		check([
			Case(text: "foo bar", cursor: 0, keys: "dwp", expectedText: "bfoo ar", expectedCursor: 4, line: #line),
			Case(text: "foo bar", cursor: 0, keys: "ywP", expectedText: "foo foo bar", expectedCursor: 3, line: #line),
			Case(text: "ab", cursor: 0, keys: "xp", expectedText: "ba", expectedCursor: 1, line: #line),
			Case(text: "one\ntwo", cursor: 0, keys: "yyp", expectedText: "one\none\ntwo", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo", cursor: 0, keys: "ddp", expectedText: "two\none", expectedCursor: 4, isMultiline: true, line: #line),
			Case(text: "one\ntwo", cursor: 4, keys: "yy2P", expectedText: "one\ntwo\ntwo\ntwo", expectedCursor: 4, isMultiline: true, line: #line),
			// `yy` leaves the cursor where it was, as `yw` does.
			Case(text: "hello", cursor: 3, keys: "yy", expectedText: "hello", expectedCursor: 3, line: #line),
		])
	}

	func testVisualModeCrossingItsStart() {
		check([
			Case(text: "abcdef", cursor: 3, keys: "vhhd", expectedText: "aef", expectedCursor: 1, line: #line),
			Case(text: "abcdef", cursor: 3, keys: "vlhhhd", expectedText: "aef", expectedCursor: 1, line: #line),
			Case(text: "abcdef", cursor: 3, keys: "vllhhhhd", expectedText: "aef", expectedCursor: 1, line: #line),
			Case(text: "abcdef", cursor: 3, keys: "vhhhhhhhd", expectedText: "ef", expectedCursor: 0, line: #line),
			Case(text: "abcdef", cursor: 3, keys: "vlllllly$p", expectedText: "abcdefdef", expectedCursor: 8, line: #line),
			Case(text: "abcdef", cursor: 2, keys: "vlcX<Esc>", expectedText: "abXef", expectedCursor: 2, line: #line),
		])
	}

	func testCursorAfterInsertCommandsAndEsc() {
		check([
			Case(text: "abc", cursor: 1, keys: "A", expectedText: "abc", expectedCursor: 3, expectedMode: .insert, line: #line),
			Case(text: "  abc", cursor: 4, keys: "I", expectedText: "  abc", expectedCursor: 2, expectedMode: .insert, line: #line),
			Case(text: "ab\ncd", cursor: 0, keys: "o", expectedText: "ab\n\ncd", expectedCursor: 3, expectedMode: .insert, isMultiline: true, line: #line),
			Case(text: "ab\ncd", cursor: 4, keys: "O", expectedText: "ab\n\ncd", expectedCursor: 3, expectedMode: .insert, isMultiline: true, line: #line),
			// Esc from Insert lands one character left, never past the line's start.
			Case(text: "abc", cursor: 1, keys: "i<Esc>", expectedText: "abc", expectedCursor: 0, line: #line),
			Case(text: "abc", cursor: 1, keys: "a<Esc>", expectedText: "abc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 0, keys: "A<Esc>", expectedText: "abc", expectedCursor: 2, line: #line),
			Case(text: "abc", cursor: 0, keys: "i<Esc>", expectedText: "abc", expectedCursor: 0, line: #line),
			Case(text: "ab\ncd", cursor: 3, keys: "I<Esc>", expectedText: "ab\ncd", expectedCursor: 3, isMultiline: true, line: #line),
			Case(text: "abc", cursor: 2, keys: "ixy<Esc>", expectedText: "abxyc", expectedCursor: 3, line: #line),
		])
	}

	func testAnEmptyFieldTakesEveryCommandQuietly() {
		for keys in ["x", "X", "dd", "dw", "db", "de", "d$", "D", "p", "P", "yy", "yw", "v", "vd", "vy", "u", "<C-r>", ".", "$", "^", "0", "w", "b", "e", "G", "gg", "h", "l", "3x", "s<Esc>", "S<Esc>", "C<Esc>", "cw<Esc>", "cc<Esc>", "A<Esc>", "I<Esc>", "a<Esc>"] {
			var editor = VimEditor(text: "")
			Self.type(keys, into: &editor)
			XCTAssertEqual(editor.text, "", keys)
			XCTAssertEqual(editor.cursor, 0, keys)
			XCTAssertEqual(editor.mode, keys == "v" ? .visual : .normal, keys)
			XCTAssertEqual(editor.selection, 0 ..< 0, keys)
		}
	}

	func testKeysThatTakeACharacterNeverRunIt() {
		check([
			// `f` `t` `F` `T` find a character on the line, `;` `,` repeat the find.
			Case(text: "one two three", cursor: 0, keys: "ft", expectedText: "one two three", expectedCursor: 4, line: #line),
			Case(text: "one two three", cursor: 0, keys: "tt", expectedText: "one two three", expectedCursor: 3, line: #line),
			Case(text: "one two three", cursor: 12, keys: "Ft", expectedText: "one two three", expectedCursor: 8, line: #line),
			Case(text: "one two three", cursor: 12, keys: "Tt", expectedText: "one two three", expectedCursor: 9, line: #line),
			Case(text: "a,b,c,d", cursor: 0, keys: "f,;", expectedText: "a,b,c,d", expectedCursor: 3, line: #line),
			Case(text: "a,b,c,d", cursor: 0, keys: "f,;,", expectedText: "a,b,c,d", expectedCursor: 1, line: #line),
			Case(text: "a,b,c,d", cursor: 0, keys: "2f,", expectedText: "a,b,c,d", expectedCursor: 3, line: #line),
			Case(text: "one two three", cursor: 0, keys: "fz", expectedText: "one two three", expectedCursor: 0, line: #line),
			Case(text: "one, two", cursor: 0, keys: "dt,", expectedText: ", two", expectedCursor: 0, line: #line),
			Case(text: "one, two", cursor: 0, keys: "df,", expectedText: " two", expectedCursor: 0, line: #line),
			Case(text: "one, two", cursor: 0, keys: "ct,X<Esc>", expectedText: "X, two", expectedCursor: 0, line: #line),
			Case(text: "one, two", cursor: 0, keys: "df,.", expectedText: "one, two".replacingOccurrences(of: "one,", with: ""), expectedCursor: 0, line: #line),
			Case(text: "a b c", cursor: 0, keys: "vf d", expectedText: "b c", expectedCursor: 0, line: #line),
			// A digit after `f` is its character, not a count, also when `.` swaps the count.
			Case(text: "x2y2z2w", cursor: 0, keys: "df2", expectedText: "y2z2w", expectedCursor: 0, line: #line),
			Case(text: "x2y2z2w", cursor: 0, keys: "df22.", expectedText: "w", expectedCursor: 0, line: #line),
			Case(text: "abc", cursor: 0, keys: "vlrx", expectedText: "xxc", expectedCursor: 0, line: #line),
			// `r` replaces the character under the cursor; `3r` the next three.
			Case(text: "abc", cursor: 1, keys: "rx", expectedText: "axc", expectedCursor: 1, line: #line),
			Case(text: "abcd", cursor: 0, keys: "3rx", expectedText: "xxxd", expectedCursor: 2, line: #line),
			Case(text: "abc", cursor: 1, keys: "5rx", expectedText: "abc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 0, keys: "rxl.", expectedText: "xxc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 0, keys: "r<Esc>", expectedText: "abc", expectedCursor: 0, line: #line),
			// Marks, macros and registers are not kept, but their character is still theirs, not a command.
			Case(text: "abc", cursor: 1, keys: "mx", expectedText: "abc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 1, keys: "qx", expectedText: "abc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 1, keys: "@x", expectedText: "abc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 1, keys: "\"ax", expectedText: "ac", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 1, keys: "'x", expectedText: "abc", expectedCursor: 1, line: #line),
			Case(text: "abc", cursor: 1, keys: "`x", expectedText: "abc", expectedCursor: 1, line: #line),
		])
		var editor = Self.editor("abc", cursor: 0)
		Self.type("f", into: &editor)
		XCTAssertEqual(editor.pendingText, "f", "the find waits for its character")
		Self.type("<Esc>", into: &editor)
		XCTAssertEqual(editor.pendingText, "")
		XCTAssertEqual(editor.handle(.escape), .leave)
		Self.type("f", into: &editor)
		XCTAssertEqual(editor.handle(.enter), .handled, "⏎ after `f` drops the find; it does not submit the field")
		XCTAssertEqual(editor.pendingText, "")
	}

	func testAOneLineFieldNeverTakesANewlineFromTheRegister() {
		var lines = Self.editor("one\ntwo", cursor: 0, isMultiline: true)
		Self.type("2yy", into: &lines)
		var field = VimEditor(text: "x")
		field.register = lines.register
		Self.type("p", into: &field)
		XCTAssertFalse(field.text.contains("\n"), "a task title or a query is one line")
		XCTAssertEqual(field.text, "xone two")
	}
}
