import Foundation
import XCTest
@testable import VaultdayCore

/// The Day log tab against the mock vault: reading a day, and appending a
/// line through the core's writer with the exact bytes the format fixes.
@MainActor
final class DayLogModelTests: XCTestCase {
	private let now: () -> Date = {
		var parts = DateComponents()
		parts.year = 2026
		parts.month = 10
		parts.day = 23
		parts.hour = 9
		parts.minute = 48
		return Calendar.current.date(from: parts) ?? Date()
	}

	func testTheTabReadsADaysLinesFromTheCore() {
		let model = DayLogModel(now: now)
		model.load(workspacePath: TestFixtures.mockVault.path, date: "2026-10-22")

		XCTAssertNil(model.errorMessage)
		XCTAssertEqual(model.path, "Log/2026-10-22.md")
		XCTAssertEqual(model.lines.map(\.time), ["08:45", "09:30", "11:15", "14:00"])
		XCTAssertEqual(model.lines.map(\.kind), ["done", "session", "done", "note"])
		XCTAssertEqual(model.lines[2].agent, "claude-fable-5-1")
		XCTAssertEqual(model.lines[0].task, "task-1")
		XCTAssertEqual(model.caption, "Log/2026-10-22.md · 4 lines")
	}

	func testAKindChipFiltersTheLines() {
		let model = DayLogModel(now: now)
		model.load(workspacePath: TestFixtures.mockVault.path, date: "2026-10-22")

		model.kindFilter = .done
		XCTAssertEqual(model.visibleLines.map(\.time), ["08:45", "11:15"])
		model.kindFilter = .blocker
		XCTAssertEqual(model.visibleLines, [])
		model.kindFilter = nil
		XCTAssertEqual(model.visibleLines.count, 4)
	}

	func testTheKindWordTheWriterAddsIsNotShownTwice() {
		XCTAssertEqual(DayLogText.display("decision · keep it dev only", kind: "decision"), "keep it dev only")
		XCTAssertEqual(DayLogText.display("Started · Build the script", kind: "session"), "Started · Build the script", "another verb is part of the line")
		XCTAssertEqual(DayLogText.display("plain text", kind: "note"), "plain text")
		XCTAssertEqual(DayLogText.display("decision · x", kind: nil), "decision · x")
	}

	func testADayWithoutALogIsEmptyAndTheDaysStep() {
		let model = DayLogModel(now: now)
		model.load(workspacePath: TestFixtures.mockVault.path, date: "2026-10-22")

		model.showDay(offset: -1)
		XCTAssertEqual(model.date, "2026-10-21")
		XCTAssertEqual(model.lines.count, 6)
		model.showDay(offset: -20)
		XCTAssertEqual(model.lines, [])
		XCTAssertEqual(model.caption, "Log/2026-10-01.md · 0 lines")
	}

	func testAppendWritesTheShorthandLineByteForByteAndShowsIt() throws {
		let copy = try TestFixtures.copyOfMockVault(self)
		let model = DayLogModel(now: now)
		model.load(workspacePath: copy.path)
		XCTAssertEqual(model.date, "2026-10-23")
		XCTAssertEqual(model.lines, [])
		model.contextStory = "WEB-142"
		model.contextTask = "task-2"
		model.draft = "decision: keep the QA redirect group dev and test only"
		XCTAssertEqual(model.draftKind, "decision")

		XCTAssertTrue(model.append())

		let line = "- 09:48 decision · keep the QA redirect group dev and test only [story:: WEB-142] [kind:: decision] [task:: task-2]"
		XCTAssertEqual(model.lastWrittenLine, line)
		XCTAssertEqual(model.draft, "")
		XCTAssertEqual(model.lines.map(\.time), ["09:48"])
		XCTAssertEqual(model.lines.first?.kind, "decision")
		XCTAssertEqual(model.lines.first?.text, "keep the QA redirect group dev and test only")
		// The new day's log note is created with the header the core writes, then the line, and nothing else.
		let written = try String(contentsOf: copy.appending(path: "Log/2026-10-23.md"), encoding: .utf8)
		XCTAssertEqual(written, "---\ntype: log\ndate: 2026-10-23\n---\n# 2026-10-23\n\n## Log\n\(line)\n")
	}

	func testAppendWithoutAKindWordIsANoteAndAppendsToAnExistingLog() throws {
		let copy = try TestFixtures.copyOfMockVault(self)
		let logFile = copy.appending(path: "Log/2026-10-22.md")
		let before = try String(contentsOf: logFile, encoding: .utf8)
		let model = DayLogModel(now: { DayText.date(from: "2026-10-22").flatMap { Calendar.current.date(bySettingHour: 16, minute: 5, second: 0, of: $0) } ?? Date() })
		model.load(workspacePath: copy.path)
		model.draft = "call at 10:30 about the invoice"

		XCTAssertTrue(model.append())

		XCTAssertEqual(model.lastWrittenLine, "- 16:05 note · call at 10:30 about the invoice [kind:: note]")
		let after = try String(contentsOf: logFile, encoding: .utf8)
		let added = after.replacingOccurrences(of: before, with: "")
		XCTAssertTrue(after.contains("- 16:05 note · call at 10:30 about the invoice [kind:: note]\n"))
		XCTAssertEqual(model.lines.count, 5)
		XCTAssertEqual(model.lines.last?.time, "16:05")
		XCTAssertFalse(added.isEmpty)
		for original in before.split(separator: "\n") {
			XCTAssertTrue(after.contains(original), "line kept: \(original)")
		}
	}

	func testAnEmptyOrBrokenLineIsRefusedAndNothingIsWritten() throws {
		let copy = try TestFixtures.copyOfMockVault(self)
		let model = DayLogModel(now: now)
		model.load(workspacePath: copy.path)

		model.draft = "   "
		XCTAssertFalse(model.append())
		XCTAssertNotNil(model.errorMessage)

		model.draft = "note: a | pipe"
		XCTAssertFalse(model.append())
		XCTAssertEqual(model.draft, "note: a | pipe", "the draft stays for correction")
		XCTAssertFalse(FileManager.default.fileExists(atPath: copy.appending(path: "Log/2026-10-23.md").path))
	}
}
