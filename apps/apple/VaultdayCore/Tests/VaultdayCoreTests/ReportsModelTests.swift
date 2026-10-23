import Foundation
import XCTest
@testable import VaultdayCore

/// The Reports tab: range presets, the bundle per billing model (read from the
/// core, against the synthetic mock vault), the copy payloads, and the one
/// file Open in Claude Code writes.
@MainActor
final class ReportsModelTests: XCTestCase {
	private static let calendar: Calendar = {
		var calendar = Calendar(identifier: .gregorian)
		calendar.timeZone = .current
		return calendar
	}()

	private func day(_ text: String) -> Date {
		DayText.date(from: text) ?? Date()
	}

	private func model(now: String = "2026-10-22", vault: URL = TestFixtures.mockVault, copied: Copied? = nil) -> ReportsModel {
		let model = ReportsModel(now: { self.day(now) }, calendar: Self.calendar, copyText: { copied?.texts.append($0) })
		model.attach(source: (try? openWorkspace(path: vault.path)).map(CoreReportSource.init))
		return model
	}

	@MainActor
	private final class Copied {
		var texts: [String] = []
	}

	private func goldenText(_ name: String) throws -> String {
		try String(contentsOf: TestFixtures.repoRoot.appending(path: "core/fixtures/golden/\(name)"), encoding: .utf8)
	}

	/// A copy of the mock vault with a billing model and three logged sessions on WEB-142.
	private func vault(billing: String?) throws -> URL {
		let root = try TestFixtures.copyOfMockVault(self)
		if let billing {
			let settings = root.appending(path: ".vaultday/settings.json")
			var object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: settings)) as? [String: Any])
			object["billing"] = ["model": billing]
			try JSONSerialization.data(withJSONObject: object).write(to: settings)
		}
		let log = root.appending(path: "Log/2026-10-22.md")
		let text = try String(contentsOf: log, encoding: .utf8)
		let sessions = """
		- 15:00 Wire the payment form to the gateway sandbox · 25m [kind:: session] [id:: s1] [story:: WEB-142] [task:: task-2]
		- 15:30 Handle the declined-card response · 1h 20m [kind:: session] [id:: s2] [story:: WEB-142] [task:: task-4]

		"""
		try text.replacingOccurrences(of: "\n## Journal", with: "\n" + sessions + "\n## Journal").write(to: log, atomically: true, encoding: .utf8)
		return root
	}

	private func snapshot(_ root: URL) throws -> [String: Data] {
		var files: [String: Data] = [:]
		let walker = try XCTUnwrap(FileManager.default.enumerator(atPath: root.path))
		for case let relative as String in walker {
			var isFolder: ObjCBool = false
			let path = root.appending(path: relative).path
			if FileManager.default.fileExists(atPath: path, isDirectory: &isFolder), !isFolder.boolValue {
				files["/" + relative] = try Data(contentsOf: URL(fileURLWithPath: path))
			}
		}
		return files
	}

	// MARK: Ranges

	func testPresetsGiveMondayToSundayWeeksAndTheWholeMonth() {
		let now = day("2026-10-22")
		XCTAssertEqual(ReportRange.range(for: .thisWeek, now: now, calendar: Self.calendar), ReportRange(from: "2026-10-19", to: "2026-10-25"))
		XCTAssertEqual(ReportRange.range(for: .lastWeek, now: now, calendar: Self.calendar), ReportRange(from: "2026-10-12", to: "2026-10-18"))
		XCTAssertEqual(ReportRange.range(for: .thisMonth, now: now, calendar: Self.calendar), ReportRange(from: "2026-10-01", to: "2026-10-31"))
		XCTAssertNil(ReportRange.range(for: .custom, now: now, calendar: Self.calendar), "the days of Custom are picked")
	}

	func testPresetsAcrossAMonthAndYearEdge() {
		let sunday = day("2026-11-01")
		XCTAssertEqual(ReportRange.range(for: .thisWeek, now: sunday, calendar: Self.calendar), ReportRange(from: "2026-10-26", to: "2026-11-01"), "a Sunday closes its week")
		XCTAssertEqual(ReportRange.range(for: .thisMonth, now: sunday, calendar: Self.calendar), ReportRange(from: "2026-11-01", to: "2026-11-30"))
		let newYear = day("2027-01-01")
		XCTAssertEqual(ReportRange.range(for: .lastWeek, now: newYear, calendar: Self.calendar), ReportRange(from: "2026-12-21", to: "2026-12-27"))
		XCTAssertEqual(ReportRange.range(for: .thisMonth, now: day("2026-02-10"), calendar: Self.calendar), ReportRange(from: "2026-02-01", to: "2026-02-28"))
	}

	func testSelectingAPresetSetsTheRangeAndReadsTheReport() throws {
		let model = model()
		model.show(subject: "WEB-142")
		model.select(preset: .lastWeek)
		XCTAssertEqual(model.range, ReportRange(from: "2026-10-12", to: "2026-10-18"))
		XCTAssertEqual(model.rangeText, "2026-10-12 to 2026-10-18")
		let bundle = try XCTUnwrap(model.bundle, model.errorMessage ?? "no bundle")
		XCTAssertEqual(bundle.range, model.range)
		XCTAssertEqual(bundle.logLineCount, 0, "the mock vault's lines are in the week after")
		model.select(preset: .thisWeek)
		XCTAssertEqual(try XCTUnwrap(model.bundle).logLineCount, 6)
	}

	func testACustomRangeThatEndsBeforeItStartsIsSaidNotSent() throws {
		let model = model()
		model.show(subject: "WEB-142")
		model.setCustomRange(from: day("2026-10-22"), to: day("2026-10-21"))
		XCTAssertEqual(model.preset, .custom)
		XCTAssertNil(model.bundle)
		XCTAssertEqual(model.errorMessage, "The range ends before it starts")
		model.setCustomRange(from: day("2026-10-22"), to: day("2026-10-22"))
		XCTAssertEqual(model.rangeText, "2026-10-22")
		XCTAssertNotNil(model.bundle)
		XCTAssertNil(model.errorMessage)
	}

	// MARK: The bundle

	func testTheBundleIsTheCoresForTheSameStoryAndRange() throws {
		let model = model()
		model.show(subject: "WEB-142")
		model.setCustomRange(from: day("2026-10-21"), to: day("2026-10-22"))
		let bundle = try XCTUnwrap(model.bundle, model.errorMessage ?? "no bundle")
		XCTAssertEqual(bundle.json, try goldenText("report-web-142.json"), "the same JSON the core pins and the CLI fills authorship into")
		XCTAssertEqual(bundle.markdown, try goldenText("report-web-142.md"), "the markdown, byte for byte")
		XCTAssertEqual(bundle.stories.map(\.heading), ["WEB-142 · Checkout redesign"])
		XCTAssertEqual(bundle.stories.first?.nextSteps.text, "4 · 3 open · 1 in progress")
		XCTAssertEqual(bundle.logGroups.map(\.kind), ["status", "decision", "session", "blocker", "done"])
		XCTAssertEqual(bundle.logGroups.map(\.title), ["Status", "Decision", "Session", "Blocker", "Done"])
		XCTAssertEqual(ReportBundle.authorshipText, "Authorship: computed by the CLI")
		XCTAssertNil(bundle.billing)
		XCTAssertEqual(bundle.hours, .logged(minutes: 0, byStory: []))
	}

	func testAClientReportCoversItsStoriesAndTheWorkspaceListsItsClients() throws {
		let model = model()
		XCTAssertEqual(model.clients.first, "Mock Studio")
		XCTAssertTrue(model.clients.contains("Website"))
		model.show(subject: "Website", load: true)
		let bundle = try XCTUnwrap(model.bundle, model.errorMessage ?? "no bundle")
		XCTAssertTrue(bundle.isClient)
		XCTAssertEqual(bundle.stories.compactMap(\.number), ["WEB-142", "WEB-157"])
		XCTAssertEqual(try XCTUnwrap(model.claudeCodeLink).absoluteString, "vaultday://claude-code/client/Website")
		model.show(subject: "WEB-142", load: true)
		XCTAssertEqual(try XCTUnwrap(model.claudeCodeLink).absoluteString, "vaultday://claude-code/story/WEB-142")
	}

	// MARK: Billing models

	func testWithoutABillingModelHoursAreLoggedTimeOnly() throws {
		let model = model(vault: try vault(billing: nil))
		model.show(subject: "WEB-142")
		model.setCustomRange(from: day("2026-10-21"), to: day("2026-10-22"))
		guard case let .logged(minutes, byStory)? = model.bundle?.hours else { return XCTFail("\(String(describing: model.bundle?.hours))") }
		XCTAssertEqual(minutes, 105)
		XCTAssertEqual(byStory, [ReportStoryMinutes(story: "WEB-142", minutes: 105)])
	}

	func testHourlyShowsBillableHoursByStoryAndNonBillableApart() throws {
		let root = try vault(billing: "hourly")
		let note = root.appending(path: "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md")
		let text = try String(contentsOf: note, encoding: .utf8)
		try text.replacingOccurrences(of: "[estimate:: 45m]", with: "[estimate:: 45m] [billable:: false]").write(to: note, atomically: true, encoding: .utf8)
		let model = model(vault: root)
		model.show(subject: "WEB-142")
		model.setCustomRange(from: day("2026-10-21"), to: day("2026-10-22"))
		guard case let .hourly(rounding, billableMinutes, billable, nonBillableMinutes, nonBillable)? = model.bundle?.hours else {
			return XCTFail("\(String(describing: model.bundle?.hours))")
		}
		XCTAssertEqual(rounding, 15)
		XCTAssertEqual(billableMinutes, 30, "25 minutes round up to the 15 minute increment")
		XCTAssertEqual(billable, [ReportStoryMinutes(story: "WEB-142", minutes: 30)])
		XCTAssertEqual(nonBillableMinutes, 80)
		XCTAssertEqual(nonBillable, [ReportStoryMinutes(story: "WEB-142", minutes: 80)])
		XCTAssertEqual(model.bundle?.billing, .hourly)
		XCTAssertFalse(try XCTUnwrap(model.bundle?.hours).hasMoneySection)
	}

	func testFixedShowsQuotedAgainstDeliveredPerTask() throws {
		let model = model(vault: try vault(billing: "fixed"))
		model.show(subject: "WEB-142")
		model.setCustomRange(from: day("2026-10-21"), to: day("2026-10-22"))
		guard case let .fixed(quoted, delivered, overrun, tasks)? = model.bundle?.hours else { return XCTFail("\(String(describing: model.bundle?.hours))") }
		XCTAssertEqual([quoted, delivered, overrun], [165, 105, 35])
		let declined = try XCTUnwrap(tasks.first { $0.task == "task-4" })
		XCTAssertEqual(declined.summary, "Quoted 45m · delivered 1h 20m · 35m unpaid")
		let wiring = try XCTUnwrap(tasks.first { $0.task == "task-2" })
		XCTAssertEqual(wiring.summary, "Quoted 2h · delivered 25m")
	}

	func testFTEShowsLoggedHoursAndNeverTrackedTimeOrMoney() throws {
		let model = model(vault: try vault(billing: "fte"))
		for subject in ["WEB-142", "Website", "Mock Studio"] {
			model.show(subject: subject)
			model.setCustomRange(from: day("2026-10-01"), to: day("2026-10-31"))
			let bundle = try XCTUnwrap(model.bundle, model.errorMessage ?? "no bundle")
			guard case let .fte(minutes, _) = bundle.hours else { return XCTFail("\(bundle.hours)") }
			XCTAssertEqual(minutes, 105)
			XCTAssertFalse(bundle.hours.hasMoneySection)
			for text in [bundle.json, bundle.markdown] {
				let lower = text.lowercased()
				for forbidden in ["tracked", "billable", "quoted", "overrun", "currency"] {
					XCTAssertFalse(lower.contains(forbidden), "\(subject): \(forbidden)")
				}
			}
		}
	}

	// MARK: Copy and the one write

	func testCopyPutsExactlyTheBundlesTextOnThePasteboard() throws {
		let copied = Copied()
		let model = model(copied: copied)
		XCTAssertNil(model.jsonPayload, "nothing to copy before a report is read")
		model.copyJSON()
		XCTAssertTrue(copied.texts.isEmpty)
		model.show(subject: "WEB-142")
		model.setCustomRange(from: day("2026-10-21"), to: day("2026-10-22"))
		model.copyJSON()
		model.copyMarkdown()
		XCTAssertEqual(copied.texts, [try goldenText("report-web-142.json"), try goldenText("report-web-142.md")])
		XCTAssertEqual(model.copyNotice, "Copied as markdown")
	}

	func testReadingAndCopyingWriteNothingAndOpenInClaudeCodeWritesOnlyTheReportFile() throws {
		let root = try TestFixtures.copyOfMockVault(self)
		let copied = Copied()
		let model = model(vault: root, copied: copied)
		let before = try snapshot(root)
		model.show(subject: "WEB-142")
		model.setCustomRange(from: day("2026-10-21"), to: day("2026-10-22"))
		model.select(preset: .thisMonth)
		model.copyJSON()
		model.copyMarkdown()
		XCTAssertEqual(try snapshot(root), before, "reading and copying a report change no file")

		model.setCustomRange(from: day("2026-10-21"), to: day("2026-10-22"))
		let written = try XCTUnwrap(model.writeForClaudeCode(), model.errorMessage ?? "not written")
		let expected = "Projects/Website/WEB-142 Checkout redesign/_agent/reports/2026-10-21-2026-10-22.json"
		XCTAssertEqual(written.relativePath, expected)
		XCTAssertEqual(written.absolutePath, root.appending(path: expected).path)
		XCTAssertFalse(written.relativePath.hasPrefix("Log/"))
		let after = try snapshot(root)
		XCTAssertEqual(Set(after.keys).subtracting(before.keys), ["/" + expected])
		XCTAssertEqual(after.filter { before[$0.key] != nil && before[$0.key] != $0.value }.count, 0, "no other file changed")
		XCTAssertEqual(String(decoding: try XCTUnwrap(after["/" + expected]), as: UTF8.self), try XCTUnwrap(model.jsonPayload))
		model.copyWrittenPath()
		XCTAssertEqual(copied.texts.last, written.absolutePath, "the path is copyable")
	}

	func testNothingIsWrittenWithoutAReportOrForABadRange() throws {
		let root = try TestFixtures.copyOfMockVault(self)
		let model = model(vault: root)
		let before = try snapshot(root)
		XCTAssertNil(model.writeForClaudeCode(), "no subject")
		model.show(subject: "WEB-142")
		XCTAssertNil(model.writeForClaudeCode(), "no report read yet")
		model.setCustomRange(from: day("2026-10-22"), to: day("2026-10-21"))
		XCTAssertNil(model.writeForClaudeCode(), "a reversed range")
		model.show(subject: "Nobody", load: true)
		XCTAssertNotNil(model.errorMessage)
		XCTAssertNil(model.writeForClaudeCode())
		XCTAssertEqual(try snapshot(root), before)
	}

	// MARK: The story panel

	func testTheStoryPanelReadsItsReportWhenTheTabFirstShowsAndAfterEdits() throws {
		let story = StoryModel(now: { self.day("2026-10-22") })
		story.attach(workspacePath: TestFixtures.mockVault.path)
		story.open(number: "WEB-142")
		XCTAssertEqual(story.reports.subject, "WEB-142")
		XCTAssertNil(story.reports.bundle, "Hours is the open tab")
		story.showTab(.reports)
		XCTAssertEqual(story.reports.bundle?.stories.compactMap(\.number), ["WEB-142"])
		story.showTab(.hours)
		story.showTab(place: 2)
		XCTAssertNotNil(story.reports.bundle)
		story.open(number: "WEB-157")
		XCTAssertEqual(story.reports.subject, "WEB-157")
		XCTAssertNil(story.reports.bundle, "a new story starts without the last one's report")
		XCTAssertEqual(StoryPanelTab.allCases.filter(\.isPlanned), [.claudeCode], "Reports is no longer planned")
	}

	// MARK: Keys

	func testTheReportKeysAreBoundInBothPanelsAndNowhereElse() throws {
		let keymap = try Keymap.parse(String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)).get()
		for (scheme, json, markdown, claude) in [
			(KeyScheme.vim, ["y", "j"], ["y", "m"], ["y", "c"]),
			(KeyScheme.standard, ["shift+cmd+j"], ["shift+cmd+m"], ["alt+cmd+c"]),
		] {
			let dispatcher = KeyDispatcher(keymap: keymap, scheme: scheme)
			for pane in [Pane.storyPanel, Pane.sidePanel] {
				XCTAssertEqual(press(dispatcher, json, in: pane), .action(.copyReportJSON), "\(scheme) \(pane)")
				XCTAssertEqual(press(dispatcher, markdown, in: pane), .action(.copyReportMarkdown), "\(scheme) \(pane)")
				XCTAssertEqual(press(dispatcher, claude, in: pane), .action(.openReportInClaudeCode), "\(scheme) \(pane)")
			}
			XCTAssertNotEqual(press(dispatcher, json, in: .today), .action(.copyReportJSON), "\(scheme): only while a panel holds focus")
			XCTAssertNotNil(dispatcher.title(for: .copyReportJSON), "every report action has a label")
			XCTAssertNotNil(dispatcher.title(for: .copyReportMarkdown))
			XCTAssertNotNil(dispatcher.title(for: .openReportInClaudeCode))
		}
		XCTAssertEqual(KeyAction(name: "side-panel.tab.8"), .showSidePanelTab(8))
		XCTAssertEqual(KeyAction(name: "report.copy-json"), .copyReportJSON)
		XCTAssertTrue(KeyAction.copyReportMarkdown.isReportAction)
	}

	private func press(_ dispatcher: KeyDispatcher, _ strokes: [String], in pane: Pane) -> KeyResolution {
		var outcome: KeyResolution = .unbound
		for stroke in strokes {
			let parts = stroke.split(separator: "+").map(String.init)
			var modifiers: KeyModifiers = []
			for part in parts.dropLast() {
				switch part {
				case "shift": modifiers.insert(.shift)
				case "cmd": modifiers.insert(.command)
				case "alt": modifiers.insert(.option)
				case "ctrl": modifiers.insert(.control)
				default: break
				}
			}
			outcome = dispatcher.handle(KeyStroke(key: parts.last ?? stroke, modifiers: modifiers), in: pane)
		}
		return outcome
	}
}
