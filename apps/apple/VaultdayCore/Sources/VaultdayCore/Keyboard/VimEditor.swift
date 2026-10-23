import Foundation

/// A text field's own Vim mode under the Vim motions scheme.
public enum VimMode: String, Sendable {
	case normal
	case insert
	case visual

	/// The mode as the field's status line names it.
	public var title: String {
		switch self {
		case .normal: "NORMAL"
		case .insert: "INSERT"
		case .visual: "VISUAL"
		}
	}
}

/// One key as the field's editor reads it: the typed character (`A`, `$`,
/// `^`), a control chord (`⌃R`) or a named key.
public enum VimKey: Hashable, Sendable {
	case character(Character)
	case control(Character)
	case escape
	case enter
	case backspace
	case left
	case right
	case up
	case down

	/// The key as the pending-keys line prints it.
	public var text: String {
		switch self {
		case let .character(character): String(character)
		case let .control(character): "^\(character.uppercased())"
		case .escape: "Esc"
		case .enter: "⏎"
		case .backspace: "⌫"
		case .left: "←"
		case .right: "→"
		case .up: "↑"
		case .down: "↓"
		}
	}
}

/// What the field does after a key.
public enum VimEffect: Equatable, Sendable {
	/// The editor took the key; the field shows its new text, cursor and mode.
	case handled
	/// Insert mode: the text view types the key itself, then reports back
	/// with ``VimEditor/syncInsert(text:cursor:)``.
	case passThrough
	/// ⏎ on a one-line field: the field's submit action.
	case submit
	/// Esc in Normal mode with nothing pending: the field gives the keys up,
	/// one level up the focus tree.
	case leave
}

/// The yank register: text, and whether it holds whole lines (`yy`, `dd`).
public struct VimRegister: Equatable, Sendable {
	public var text: String
	public var isLinewise: Bool

	public init(text: String = "", isLinewise: Bool = false) {
		self.text = text
		self.isLinewise = isLinewise
	}
}

/// The editing model of a Vim text field, as a pure state machine: Normal,
/// Insert and Visual modes, counts, the motions `h` `l` `w` `b` `e` `0` `$`
/// `^` (`j` `k` `gg` `G` on several lines), the operators `d` `c` `y` over a
/// motion or doubled for the line, `x` `X` `D` `C` `s` `S`, `p` `P`, `u`
/// and `⌃R`, `.` for the last change, and `i` `a` `I` `A` (`o` `O` on
/// several lines). Positions count characters. Insert mode's typing is the
/// text view's own: it reports each edit back through ``syncInsert(text:cursor:)``.
public struct VimEditor: Equatable, Sendable {
	public private(set) var characters: [Character]
	/// The cursor: on a character in Normal and Visual mode, between characters in Insert mode.
	public private(set) var cursor: Int
	public private(set) var mode: VimMode
	/// Where Visual mode started; the selection runs from here to the cursor, both included.
	public private(set) var visualAnchor = 0
	public let isMultiline: Bool
	/// The unnamed register; the field shares it with the other fields.
	public var register = VimRegister()

	/// The keys of the command being typed, for the pending-keys line.
	private var typed: [VimKey] = []
	private var count: Int?
	private var operatorKey: Character?
	private var operatorCount: Int?
	/// `g` waiting for its second key.
	private var isGPending = false
	private var undoStack: [Snapshot] = []
	private var redoStack: [Snapshot] = []
	private var lastChange: RecordedChange?
	/// The change whose Insert mode is open, kept for `.` until Esc closes it.
	private var insertInProgress: OpenInsert?
	private var isReplaying = false
	/// The column `j` and `k` try to keep.
	private var wantedColumn: Int?

	private struct Snapshot: Equatable, Sendable {
		let characters: [Character]
		let cursor: Int
	}

	/// A change as typed, for `.`: its keys, and the text its Insert mode added.
	private struct RecordedChange: Equatable, Sendable {
		let keys: [VimKey]
		var insertedText: String?
	}

	private struct OpenInsert: Equatable, Sendable {
		let keys: [VimKey]
		let start: Int
		let charactersAtStart: [Character]
	}

	private struct Motion {
		let target: Int
		let isInclusive: Bool
		let isLinewise: Bool
	}

	private enum CharacterClass {
		case space
		case word
		case punctuation
	}

	/// A field's editor; it starts in Normal mode unless the key that opened
	/// the field means "insert" (`i`, `o`, `cc`).
	public init(text: String = "", isMultiline: Bool = false, mode: VimMode = .normal) {
		characters = Array(text)
		self.isMultiline = isMultiline
		self.mode = mode
		cursor = mode == .insert ? characters.count : 0
		if mode != .insert { clampToLine() }
	}

	public var text: String { String(characters) }

	/// The keys typed toward a command not yet complete (`2d`, `g`).
	public var pendingText: String {
		typed.map(\.text).joined()
	}

	/// The characters the field shows selected: the Visual selection, or in
	/// Normal mode the one character under the block cursor.
	public var selection: Range<Int> {
		switch mode {
		case .visual:
			let start = min(visualAnchor, cursor)
			let end = min(characters.count, max(visualAnchor, cursor) + 1)
			return start ..< end
		case .normal:
			guard cursor < characters.count, characters[cursor] != "\n" else { return cursor ..< cursor }
			return cursor ..< cursor + 1
		case .insert:
			return cursor ..< cursor
		}
	}

	/// The text was set from outside (a binding changed, the field was cleared).
	public mutating func setText(_ text: String) {
		let replacement = Array(text)
		guard replacement != characters else { return }
		characters = replacement
		cursor = min(cursor, characters.count)
		if mode != .insert { clampToLine() }
	}

	/// Insert mode typed natively: the text view's text and caret after the edit.
	public mutating func syncInsert(text: String, cursor: Int) {
		characters = Array(text)
		self.cursor = min(max(0, cursor), characters.count)
	}

	/// Moves the caret to `position`, as a click does; Normal mode keeps it on a character.
	public mutating func place(cursor position: Int) {
		cursor = min(max(0, position), characters.count)
		if mode == .visual { mode = .normal }
		if mode == .normal { clampToLine() }
	}

	/// Enters Insert mode at the end of the text: the field was opened by a
	/// key that means insert.
	public mutating func startInserting() {
		guard mode != .insert else { return }
		beginChange()
		enterInsert(at: characters.count, keys: [.character("A")])
	}

	/// Feeds one key.
	public mutating func handle(_ key: VimKey) -> VimEffect {
		switch mode {
		case .insert: handleInsert(key)
		case .normal: handleNormal(key)
		case .visual: handleVisual(key)
		}
	}

	// MARK: Insert

	private mutating func handleInsert(_ key: VimKey) -> VimEffect {
		switch key {
		case .escape:
			closeInsert()
			return .handled
		case .enter where !isMultiline:
			return .submit
		default:
			return .passThrough
		}
	}

	private mutating func enterInsert(at position: Int, keys: [VimKey]) {
		cursor = min(max(0, position), characters.count)
		mode = .insert
		if !isReplaying {
			insertInProgress = OpenInsert(keys: keys, start: cursor, charactersAtStart: characters)
		}
		resetCommand()
	}

	/// Esc from Insert mode: back to Normal, the cursor one left as in Vim,
	/// and the change with what was typed kept for `.`.
	private mutating func closeInsert() {
		if let open = insertInProgress {
			lastChange = RecordedChange(keys: open.keys, insertedText: insertedText(since: open))
		}
		insertInProgress = nil
		mode = .normal
		if cursor > lineStart(of: cursor) { cursor -= 1 }
		clampToLine()
	}

	/// What Insert mode added, when it only added one run of text at its start.
	private func insertedText(since open: OpenInsert) -> String? {
		let before = open.charactersAtStart
		let added = characters.count - before.count
		guard added >= 0, open.start + added <= characters.count else { return nil }
		let head = characters[..<open.start]
		let tail = characters[(open.start + added)...]
		guard Array(head) == Array(before[..<open.start]), Array(tail) == Array(before[open.start...]) else { return nil }
		return String(characters[open.start ..< open.start + added])
	}

	// MARK: Normal

	private mutating func handleNormal(_ key: VimKey) -> VimEffect {
		if key == .escape {
			if typed.isEmpty { return .leave }
			resetCommand()
			return .handled
		}
		typed.append(key)
		switch key {
		case .enter:
			resetCommand()
			return isMultiline ? move(.character("j")) : .submit
		case .control("r"):
			redo()
			resetCommand()
			return .handled
		case .left, .backspace:
			return move(.character("h"))
		case .right:
			return move(.character("l"))
		case .up:
			return move(.character("k"))
		case .down:
			return move(.character("j"))
		case let .character(character):
			return normalCharacter(character)
		default:
			resetCommand()
			return .handled
		}
	}

	private mutating func normalCharacter(_ character: Character) -> VimEffect {
		if let digit = character.wholeNumberValue, character.isASCII, digit > 0 || hasCount {
			if operatorKey != nil {
				operatorCount = (operatorCount ?? 0) * 10 + digit
			} else {
				count = (count ?? 0) * 10 + digit
			}
			return .handled
		}
		if isGPending {
			isGPending = false
			if character == "g" { return motionOrOperator(.character("G"), firstLine: true) }
			resetCommand()
			return .handled
		}
		if let operatorKey {
			if character == operatorKey { return applyLinewiseOperator(operatorKey) }
			if character == "g" {
				isGPending = true
				return .handled
			}
			return motionOrOperator(.character(character), firstLine: false)
		}
		switch character {
		case "d", "c", "y":
			operatorKey = character
			return .handled
		case "g":
			isGPending = true
			return .handled
		case "i":
			return insertCommand { $0.cursor }
		case "a":
			return insertCommand { $0.lineIsEmpty ? $0.cursor : min($0.cursor + 1, $0.lineEnd(of: $0.cursor)) }
		case "I":
			return insertCommand { $0.firstNonBlank(of: $0.cursor) }
		case "A":
			return insertCommand { $0.lineEnd(of: $0.cursor) }
		case "o", "O":
			return openLine(below: character == "o")
		case "x", "X", "D", "C", "s", "S":
			return shorthand(character)
		case "p", "P":
			return paste(after: character == "p")
		case "u":
			undo()
			resetCommand()
			return .handled
		case "v":
			mode = .visual
			visualAnchor = cursor
			resetCommand()
			return .handled
		case ".":
			return repeatLastChange()
		default:
			return move(.character(character))
		}
	}

	private var hasCount: Bool {
		operatorKey != nil ? operatorCount != nil : count != nil
	}

	private var totalCount: Int {
		(count ?? 1) * (operatorCount ?? 1)
	}

	/// A motion key alone moves the cursor.
	private mutating func move(_ key: VimKey) -> VimEffect {
		defer { resetCommand() }
		guard case let .character(character) = key, let motion = motion(character, count: totalCount, forOperator: false) else {
			return .handled
		}
		if !["j", "k"].contains(character) { wantedColumn = nil }
		cursor = motion.target
		clampToLine()
		return .handled
	}

	/// After an operator, a motion names the text it acts on; `firstLine` is `gg`.
	private mutating func motionOrOperator(_ key: VimKey, firstLine: Bool) -> VimEffect {
		guard let operatorKey else {
			// `gg` alone: the first line.
			cursor = firstLine ? firstNonBlank(of: 0) : cursor
			resetCommand()
			return .handled
		}
		guard case var .character(character) = key else {
			resetCommand()
			return .handled
		}
		if firstLine { character = "g" }
		let operatorCount = totalCount
		// `cw` changes to the end of the word, not up to the next one, as in Vim.
		if operatorKey == "c", character == "w", cursor < characters.count, classOf(cursor) != .space {
			character = "e"
		}
		guard let motion = motion(character, count: operatorCount, forOperator: true) else {
			resetCommand()
			return .handled
		}
		let range: Range<Int>
		if motion.isLinewise {
			range = linesRange(from: min(cursor, motion.target), to: max(cursor, motion.target))
		} else {
			let start = min(cursor, motion.target)
			let end = max(cursor, motion.target) + (motion.isInclusive ? 1 : 0)
			range = start ..< min(end, characters.count)
		}
		let firstLine = motion.isLinewise ? lineStart(of: min(cursor, motion.target)) : range.lowerBound
		return apply(operatorKey, to: range, isLinewise: motion.isLinewise, landingAt: firstLine)
	}

	/// `dd`, `cc`, `yy`: the count's lines.
	private mutating func applyLinewiseOperator(_ operatorKey: Character) -> VimEffect {
		let start = lineStart(of: cursor)
		var end = lineEnd(of: cursor)
		for _ in 1 ..< max(1, totalCount) where end < characters.count {
			end = lineEnd(of: end + 1)
		}
		if operatorKey == "c" {
			// `cc` empties the lines and types in their place.
			let keys = typed
			beginChange()
			register = VimRegister(text: String(characters[start ..< end]), isLinewise: true)
			wantedColumn = nil
			characters.removeSubrange(start ..< end)
			enterInsert(at: start, keys: keys)
			return .handled
		}
		return apply(operatorKey, to: linesRange(from: start, to: end > start ? end - 1 : end), isLinewise: true, landingAt: start)
	}

	/// Runs `d`, `c` or `y` over `range`; a yank leaves the cursor on `landing`,
	/// the start of its first line or character.
	private mutating func apply(_ operatorKey: Character, to range: Range<Int>, isLinewise: Bool, landingAt landing: Int) -> VimEffect {
		let keys = typed
		register = VimRegister(text: isLinewise ? linewiseText(range) : String(characters[range]), isLinewise: isLinewise)
		switch operatorKey {
		case "y":
			cursor = landing
			clampToLine()
			resetCommand()
			return .handled
		case "c":
			beginChange()
			characters.removeSubrange(range)
			enterInsert(at: range.lowerBound, keys: keys)
			return .handled
		default:
			beginChange()
			characters.removeSubrange(range)
			cursor = range.lowerBound
			if isLinewise { cursor = firstNonBlank(of: min(cursor, max(0, characters.count - 1))) }
			clampToLine()
			record(keys)
			resetCommand()
			return .handled
		}
	}

	/// `x` `X` `D` `C` `s` `S`, as the operator they stand for.
	private mutating func shorthand(_ character: Character) -> VimEffect {
		let keys = typed
		let count = totalCount
		switch character {
		case "x", "X":
			let range: Range<Int>
			if character == "x" {
				guard !lineIsEmpty else {
					resetCommand()
					return .handled
				}
				range = cursor ..< min(cursor + count, lineEnd(of: cursor))
			} else {
				range = max(lineStart(of: cursor), cursor - count) ..< cursor
			}
			guard !range.isEmpty else {
				resetCommand()
				return .handled
			}
			beginChange()
			register = VimRegister(text: String(characters[range]))
			characters.removeSubrange(range)
			cursor = range.lowerBound
			clampToLine()
			record(keys)
			resetCommand()
			return .handled
		case "D", "C":
			let range = cursor ..< lineEnd(of: cursor)
			beginChange()
			register = VimRegister(text: String(characters[range]))
			characters.removeSubrange(range)
			if character == "C" {
				enterInsert(at: range.lowerBound, keys: keys)
			} else {
				cursor = range.lowerBound
				clampToLine()
				record(keys)
				resetCommand()
			}
			return .handled
		case "s":
			let range = cursor ..< min(cursor + count, lineEnd(of: cursor))
			beginChange()
			register = VimRegister(text: String(characters[range]))
			characters.removeSubrange(range)
			enterInsert(at: range.lowerBound, keys: keys)
			return .handled
		default:
			operatorKey = "c"
			return applyLinewiseOperator("c")
		}
	}

	private mutating func insertCommand(_ position: (VimEditor) -> Int) -> VimEffect {
		let keys = typed
		let target = position(self)
		beginChange()
		enterInsert(at: target, keys: keys)
		return .handled
	}

	/// `o` `O`: a new line below or above, in Insert mode; one-line fields have no lines to open.
	private mutating func openLine(below: Bool) -> VimEffect {
		guard isMultiline else {
			resetCommand()
			return .handled
		}
		let keys = typed
		beginChange()
		let position = below ? lineEnd(of: cursor) : lineStart(of: cursor)
		characters.insert("\n", at: position)
		enterInsert(at: below ? position + 1 : position, keys: keys)
		return .handled
	}

	/// `p` `P`: the register after or before the cursor, or as lines below or above.
	private mutating func paste(after: Bool) -> VimEffect {
		let keys = typed
		let count = totalCount
		guard !register.text.isEmpty else {
			resetCommand()
			return .handled
		}
		beginChange()
		let pasted = Array(String(repeating: register.text, count: count))
		if register.isLinewise, isMultiline {
			let lines = Array(Array(repeating: register.text, count: count).joined(separator: "\n"))
			if after {
				let end = lineEnd(of: cursor)
				characters.insert(contentsOf: ["\n"] + lines, at: end)
				cursor = end + 1
			} else {
				let start = lineStart(of: cursor)
				characters.insert(contentsOf: lines + ["\n"], at: start)
				cursor = start
			}
			cursor = firstNonBlank(of: cursor)
		} else {
			let position = after && !lineIsEmpty ? min(cursor + 1, characters.count) : cursor
			characters.insert(contentsOf: pasted, at: position)
			cursor = position + pasted.count - 1
		}
		clampToLine()
		record(keys)
		resetCommand()
		return .handled
	}

	/// `.`: the last change again, at the cursor; a count replaces the change's own.
	private mutating func repeatLastChange() -> VimEffect {
		let newCount = count
		resetCommand()
		guard let change = lastChange else { return .handled }
		var keys = change.keys
		if let newCount {
			let withoutCount = change.keys.drop { if case let .character(character) = $0 { character.isNumber } else { false } }
			keys = String(newCount).map { VimKey.character($0) } + withoutCount
		}
		isReplaying = true
		defer { isReplaying = false }
		for key in keys {
			_ = handle(key)
		}
		if mode == .insert {
			if let inserted = change.insertedText {
				characters.insert(contentsOf: Array(inserted), at: cursor)
				cursor += inserted.count
			}
			mode = .normal
			if cursor > lineStart(of: cursor) { cursor -= 1 }
			clampToLine()
		}
		return .handled
	}

	private mutating func record(_ keys: [VimKey]) {
		guard !isReplaying else { return }
		lastChange = RecordedChange(keys: keys, insertedText: nil)
	}

	// MARK: Visual

	private mutating func handleVisual(_ key: VimKey) -> VimEffect {
		switch key {
		case .escape:
			mode = .normal
			resetCommand()
			clampToLine()
			return .handled
		case .left, .backspace:
			return visualMove("h")
		case .right:
			return visualMove("l")
		case .up:
			return visualMove("k")
		case .down:
			return visualMove("j")
		case let .character(character):
			if let digit = character.wholeNumberValue, character.isASCII, digit > 0 || count != nil {
				count = (count ?? 0) * 10 + digit
				typed.append(key)
				return .handled
			}
			return visualCharacter(character)
		default:
			return .handled
		}
	}

	private mutating func visualCharacter(_ character: Character) -> VimEffect {
		let range = selection
		switch character {
		case "v":
			mode = .normal
			resetCommand()
			clampToLine()
		case "o":
			swap(&visualAnchor, &cursor)
		case "d", "x", "c", "y":
			register = VimRegister(text: String(characters[range]))
			mode = .normal
			if character == "y" {
				cursor = range.lowerBound
				resetCommand()
				return .handled
			}
			beginChange()
			characters.removeSubrange(range)
			if character == "c" {
				enterInsert(at: range.lowerBound, keys: [])
				insertInProgress = nil
				return .handled
			}
			cursor = range.lowerBound
			clampToLine()
			resetCommand()
		default:
			return visualMove(character)
		}
		return .handled
	}

	private mutating func visualMove(_ character: Character) -> VimEffect {
		if isGPending {
			isGPending = false
			if character == "g" { cursor = firstNonBlank(of: 0) }
			resetCommand()
			return .handled
		}
		if character == "g" {
			isGPending = true
			return .handled
		}
		if let motion = motion(character, count: count ?? 1, forOperator: false) {
			cursor = motion.target
			if !lineIsEmpty { cursor = min(cursor, lineEnd(of: cursor) - 1) }
		}
		resetCommand()
		return .handled
	}

	// MARK: Undo

	private mutating func beginChange() {
		undoStack.append(Snapshot(characters: characters, cursor: cursor))
		redoStack = []
	}

	private mutating func undo() {
		guard let snapshot = undoStack.popLast() else { return }
		redoStack.append(Snapshot(characters: characters, cursor: cursor))
		characters = snapshot.characters
		cursor = snapshot.cursor
		clampToLine()
	}

	private mutating func redo() {
		guard let snapshot = redoStack.popLast() else { return }
		undoStack.append(Snapshot(characters: characters, cursor: cursor))
		characters = snapshot.characters
		cursor = snapshot.cursor
		clampToLine()
	}

	private mutating func resetCommand() {
		typed = []
		count = nil
		operatorKey = nil
		operatorCount = nil
		isGPending = false
	}

	// MARK: Motions

	/// Where a motion key goes from the cursor; `nil` for a key that is no motion.
	private mutating func motion(_ character: Character, count: Int, forOperator: Bool) -> Motion? {
		switch character {
		case "h":
			return Motion(target: max(lineStart(of: cursor), cursor - count), isInclusive: false, isLinewise: false)
		case "l", " ":
			let limit = forOperator ? lineEnd(of: cursor) : max(lineStart(of: cursor), lineEnd(of: cursor) - 1)
			return Motion(target: min(limit, cursor + count), isInclusive: false, isLinewise: false)
		case "0":
			return Motion(target: lineStart(of: cursor), isInclusive: false, isLinewise: false)
		case "^":
			return Motion(target: firstNonBlank(of: cursor), isInclusive: false, isLinewise: false)
		case "$":
			var target = cursor
			for step in 0 ..< count {
				if step > 0, lineEnd(of: target) < characters.count { target = lineEnd(of: target) + 1 }
				target = lineEnd(of: target)
			}
			return Motion(target: max(lineStart(of: target), target - 1), isInclusive: true, isLinewise: false)
		case "w":
			var target = cursor
			for _ in 0 ..< count { target = nextWordStart(from: target) }
			if forOperator {
				// A word motion ending past the line stops at the line's end, as Vim's `dw` on the last word.
				let end = lineEnd(of: cursor)
				if target > end { target = end }
				return Motion(target: target, isInclusive: false, isLinewise: false)
			}
			return Motion(target: min(target, max(0, characters.count - 1)), isInclusive: false, isLinewise: false)
		case "b":
			var target = cursor
			for _ in 0 ..< count { target = previousWordStart(from: target) }
			return Motion(target: target, isInclusive: false, isLinewise: false)
		case "e":
			var target = cursor
			for _ in 0 ..< count { target = wordEnd(from: target) }
			return Motion(target: target, isInclusive: true, isLinewise: false)
		case "j", "k":
			guard isMultiline else { return nil }
			let column = wantedColumn ?? (cursor - lineStart(of: cursor))
			wantedColumn = column
			var lineHead = lineStart(of: cursor)
			for _ in 0 ..< count {
				if character == "j" {
					let end = lineEnd(of: lineHead)
					guard end < characters.count else { break }
					lineHead = end + 1
				} else {
					guard lineHead > 0 else { break }
					lineHead = lineStart(of: lineHead - 1)
				}
			}
			let target = min(lineHead + column, max(lineHead, lineEnd(of: lineHead) - 1))
			return Motion(target: target, isInclusive: true, isLinewise: true)
		case "G":
			let lastLine = lineStart(of: characters.count)
			return Motion(target: firstNonBlank(of: lastLine), isInclusive: true, isLinewise: true)
		case "g":
			// `gg`: the first line.
			return Motion(target: firstNonBlank(of: 0), isInclusive: true, isLinewise: true)
		default:
			return nil
		}
	}

	private func classOf(_ index: Int) -> CharacterClass {
		let character = characters[index]
		if character.isWhitespace { return .space }
		if character.isLetter || character.isNumber || character == "_" { return .word }
		return .punctuation
	}

	private func nextWordStart(from index: Int) -> Int {
		let count = characters.count
		guard index < count else { return count }
		var next = index
		let start = classOf(index)
		if start != .space {
			while next < count, classOf(next) == start { next += 1 }
		}
		while next < count, classOf(next) == .space { next += 1 }
		return next
	}

	private func previousWordStart(from index: Int) -> Int {
		var next = index - 1
		while next > 0, classOf(next) == .space { next -= 1 }
		guard next > 0 else { return 0 }
		let kind = classOf(next)
		while next > 0, classOf(next - 1) == kind { next -= 1 }
		return next
	}

	private func wordEnd(from index: Int) -> Int {
		let count = characters.count
		var next = index + 1
		while next < count, classOf(next) == .space { next += 1 }
		guard next < count else { return max(0, count - 1) }
		let kind = classOf(next)
		while next + 1 < count, classOf(next + 1) == kind { next += 1 }
		return next
	}

	// MARK: Lines

	private func lineStart(of index: Int) -> Int {
		var start = min(index, characters.count)
		while start > 0, characters[start - 1] != "\n" { start -= 1 }
		return start
	}

	/// The index of the line's newline, or the text's end: one past its last character.
	private func lineEnd(of index: Int) -> Int {
		var end = min(index, characters.count)
		while end < characters.count, characters[end] != "\n" { end += 1 }
		return end
	}

	private func firstNonBlank(of index: Int) -> Int {
		var position = lineStart(of: index)
		let end = lineEnd(of: index)
		while position < end, characters[position] == " " || characters[position] == "\t" { position += 1 }
		return position == end ? lineStart(of: index) : position
	}

	private var lineIsEmpty: Bool {
		lineStart(of: cursor) == lineEnd(of: cursor)
	}

	/// The lines from the one holding `start` to the one holding `end`,
	/// with one newline beside them so deleting them leaves no blank line.
	private func linesRange(from start: Int, to end: Int) -> Range<Int> {
		let first = lineStart(of: start)
		let last = lineEnd(of: end)
		if last < characters.count { return first ..< last + 1 }
		return max(0, first - (first > 0 ? 1 : 0)) ..< last
	}

	/// Lines as the register keeps them: without the newline that joined them to the text.
	private func linewiseText(_ range: Range<Int>) -> String {
		var lines = characters[range]
		if lines.last == "\n" {
			lines = lines.dropLast()
		} else if lines.first == "\n" {
			lines = lines.dropFirst()
		}
		return String(lines)
	}

	/// Keeps the Normal-mode cursor on a character of its line.
	private mutating func clampToLine() {
		cursor = min(max(0, cursor), characters.count)
		guard mode != .insert else { return }
		let start = lineStart(of: cursor)
		let end = lineEnd(of: cursor)
		cursor = end > start ? min(cursor, end - 1) : start
	}
}
