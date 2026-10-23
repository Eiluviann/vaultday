import Foundation
import XCTest
@testable import VaultdayCore

/// The story screen against the mock vault: the render mapping from the
/// core's flat nodes to the block tree, the Story block, the Next Steps
/// accordion, the panel tabs and their keys, and the write path, which
/// changes task lines only and never prose.
@MainActor
final class StoryScreenTests: XCTestCase {
	private static let today: () -> Date = { DayText.date(from: "2026-10-22") ?? Date() }

	private func story(_ number: String = "WEB-142", at path: String = TestFixtures.mockVault.path, lines: (any StoryLineWriting)? = nil) throws -> StoryModel {
		let model = StoryModel(now: Self.today)
		model.attach(workspacePath: path, lines: lines)
		model.open(number: number)
		XCTAssertNotNil(model.content, model.errorMessage ?? "no content")
		return model
	}

	// MARK: Render mapping

	func testTheNoteRendersAsABlockTreeWithTheAppsViewsInPlace() throws {
		let content = try XCTUnwrap(story().content)

		let headings = content.blocks.compactMap { block -> String? in
			if case let .heading(_, title, _, _) = block.kind { return title }
			return nil
		}
		XCTAssertEqual(headings, ["Background", "Implementation", "Next Steps", "Pending Input", "Notes", "Activity"])
		let topKinds = content.blocks.map(\.kind)
		XCTAssertTrue(topKinds.contains(.appView(.nextSteps)))
		XCTAssertTrue(topKinds.contains(.appView(.pendingInput)))
		XCTAssertTrue(content.hasActivityInBody, "the Activity query stands for the app's activity list")
		XCTAssertFalse("\(content.blocks)".contains("Wire the payment form"), "the accordion draws the steps")
	}

	func testMarkersAndFrontmatterAreNeverShown() throws {
		let content = try XCTUnwrap(story().content)
		var shown: [String] = []
		StoryBlock.visit(content.blocks) { block in
			switch block.kind {
			case let .paragraph(spans), let .listItem(_, spans, _), let .heading(_, _, spans, _):
				shown.append(spans.map(\.text).joined())
			default:
				break
			}
		}
		let text = shown.joined(separator: "\n")
		XCTAssertFalse(text.contains("ai:"), text)
		XCTAssertFalse(text.contains("%%"), text)
		XCTAssertFalse(text.contains("type: story"), text)
		XCTAssertFalse(text.contains("Checkout redesign\n"), "the title is the header, not a block")
	}

	func testCalloutsWikilinksAndImageEmbedsMapFromTheCore() throws {
		let content = try XCTUnwrap(story().content)

		let callout = try XCTUnwrap(content.blocks.first {
			if case .callout = $0.kind { return true }
			return false
		})
		XCTAssertEqual(callout.kind, .callout(kind: "note", title: [StorySpan(text: "Scope")], fold: .fixed))
		XCTAssertEqual(callout.children.count, 1)

		var links: [(String, StorySpan.Link)] = []
		StoryBlock.visit(content.blocks) { block in
			if case let .paragraph(spans) = block.kind {
				links += spans.compactMap { span in span.link.map { (span.text, $0) } }
			}
		}
		let sweep = try XCTUnwrap(links.first { $0.0 == "the competitor sweep" })
		guard case let .note(target, section, resolvedPath, obsidianLink) = sweep.1 else { return XCTFail("\(sweep)") }
		XCTAssertEqual(target, "Research notes")
		XCTAssertEqual(section, "Checkout research")
		XCTAssertEqual(resolvedPath, "Projects/Website/WEB-142 Checkout redesign/Research notes.md")
		XCTAssertTrue(obsidianLink?.absoluteString.hasSuffix("%23Checkout%20research") ?? false, "\(String(describing: obsidianLink))")

		var image: StoryEmbed?
		StoryBlock.visit(content.blocks) { block in
			if case let .embed(embed) = block.kind, embed.kind == .image { image = embed }
		}
		let file = try XCTUnwrap(image?.fileURL)
		XCTAssertTrue(FileManager.default.fileExists(atPath: file.path), file.path)
	}

	func testTheBodyCheckboxCarriesItsIdAndState() throws {
		let content = try XCTUnwrap(story().content)
		var checkboxes: [StoryCheckbox] = []
		StoryBlock.visit(content.blocks) { block in
			if case let .listItem(_, _, checkbox?) = block.kind { checkboxes.append(checkbox) }
		}
		XCTAssertEqual(checkboxes, [StoryCheckbox(state: .done, taskId: "task-1")])
		XCTAssertTrue(checkboxes[0].isTickable)
	}

	func testTheTreeIsBuiltFromParentIndices() {
		let paragraph = RenderNodeKind.paragraph(spans: [SpanView(text: "x", strong: true, emphasis: false, strikethrough: false, highlight: false, code: false, link: .url(url: "https://x.test"))])
		let nodes = [
			RenderNodeView(parent: nil, line: 1, kind: .list(isOrdered: false)),
			RenderNodeView(parent: 0, line: 1, kind: .listItem(marker: "-", spans: [], checkbox: CheckboxView(state: .open, taskId: nil))),
			RenderNodeView(parent: 1, line: 2, kind: .listItem(marker: "-", spans: [], checkbox: nil)),
			RenderNodeView(parent: nil, line: 4, kind: .quote),
			RenderNodeView(parent: 3, line: 4, kind: paragraph),
			// A parent that comes later is no parent: the node stays visible at the top.
			RenderNodeView(parent: 9, line: 6, kind: .rule),
		]

		let tree = StoryMapping.tree(nodes)

		XCTAssertEqual(tree.map(\.id), [0, 3, 5])
		XCTAssertEqual(tree[0].children.map(\.id), [1])
		XCTAssertEqual(tree[0].children[0].children.map(\.id), [2])
		XCTAssertEqual(tree[0].children[0].kind, .listItem(marker: "-", spans: [], checkbox: StoryCheckbox(state: .open, taskId: nil)))
		XCTAssertFalse(StoryCheckbox(state: .open, taskId: nil).isTickable, "a line without an id is read-only")
		XCTAssertEqual(tree[1].children.first?.kind, .paragraph([StorySpan(text: "x", isStrong: true, link: .url(URL(string: "https://x.test")!))]))
		XCTAssertEqual(tree[2].kind, .rule)
	}

	// MARK: Story block

	func testTheStoryBlockReadsFrontmatterAndProvenance() throws {
		let content = try XCTUnwrap(story().content)

		XCTAssertEqual(content.numberLine, "WEB-142 · issue · tracker.example.test")
		XCTAssertEqual(content.title, "Checkout redesign")
		XCTAssertEqual(content.facts.map(\.label), ["Status", "Project", "Priority", "Due", "Authorship"])
		XCTAssertEqual(content.facts.map(\.value), ["In progress", "Website", "High", "30 Oct", "Mixed · model text 20 Oct"])
		XCTAssertEqual(content.facts.last?.isModelText, true)
		XCTAssertFalse(content.facts.dropLast().contains(where: \.isModelText))
	}

	func testAStoryWithoutValuesSaysNotSet() throws {
		let content = try XCTUnwrap(story("ACME-12").content)
		XCTAssertEqual(content.facts.map(\.value), ["Done", "Acme portal", "Medium", "Not set", "Human"])
		XCTAssertFalse(content.hasActivityInBody)
	}

	func testHoursKeepTheThreeNumbersApart() {
		let hours = StoryHours(trackedMinutes: 220, loggedMinutes: 120, estimateMinutes: 360, remainingMinutes: 240, overtimeMinutes: 0, isEstimateFromTasks: false)
		XCTAssertEqual(hours.trackedText, "3h 40m")
		XCTAssertEqual(hours.loggedText, "2h")
		XCTAssertEqual(hours.estimateText, "6h")
		XCTAssertEqual(hours.remainingText, "4h")
		XCTAssertEqual(hours.loggedShare ?? 0, 1.0 / 3.0, accuracy: 0.001)

		let over = StoryHours(trackedMinutes: 0, loggedMinutes: 90, estimateMinutes: 60, remainingMinutes: 0, overtimeMinutes: 30, isEstimateFromTasks: true)
		XCTAssertEqual(over.estimateText, "1h from tasks")
		XCTAssertEqual(over.remainingText, "30m over")
		XCTAssertTrue(over.isOver)

		let none = StoryHours(trackedMinutes: 15, loggedMinutes: 0, estimateMinutes: nil, remainingMinutes: 0, overtimeMinutes: 0, isEstimateFromTasks: false)
		XCTAssertEqual(none.estimateText, "No estimate")
		XCTAssertNil(none.loggedShare)
	}

	// MARK: Accordion

	private func step(_ id: String, done: Bool) -> StoryStep {
		StoryStep(id: id, taskId: id, text: id, state: done ? .done : .open, depth: 0, scheduled: nil, due: nil, estimateMinutes: nil, completion: nil, assignedTo: nil)
	}

	func testTheAccordionOpensAtTheFirstOpenStepAndCountsTheDoneAbove() {
		let accordion = NextStepsAccordion(steps: [step("a", done: true), step("b", done: true), step("c", done: false), step("d", done: true)])
		XCTAssertEqual(accordion.initialScrollId, "c")
		XCTAssertEqual(accordion.completedAbove, 2, "a done step below the first open one stays in place and is not counted")
		XCTAssertEqual(accordion.completedCaption, "2 completed steps above")
		XCTAssertEqual(accordion.steps.map(\.id), ["a", "b", "c", "d"], "the note's order is kept")

		XCTAssertNil(NextStepsAccordion(steps: [step("a", done: false)]).completedCaption)
		XCTAssertEqual(NextStepsAccordion(steps: [step("a", done: true), step("b", done: false)]).completedCaption, "1 completed step above")
		XCTAssertEqual(NextStepsAccordion(steps: [step("a", done: true)]).initialScrollId, "a")
		XCTAssertNil(NextStepsAccordion(steps: []).initialScrollId)
	}

	func testOnlyTheSelectedOrHoveredStepShowsItsControls() {
		var accordion = NextStepsAccordion(steps: [step("a", done: false), step("b", done: false), step("c", done: false)])
		XCTAssertNil(accordion.expandedId)
		XCTAssertFalse(accordion.steps.contains(where: accordion.showsControls))

		accordion.select(index: 1)
		XCTAssertEqual(accordion.steps.filter(accordion.showsControls).map(\.id), ["b"])

		accordion.hover(id: "c")
		XCTAssertEqual(accordion.steps.filter(accordion.showsControls).map(\.id), ["c"], "the hovered step wins while the pointer is on it")

		accordion.hover(id: nil)
		XCTAssertEqual(accordion.expandedId, "b")
		XCTAssertEqual(accordion.selectedIndex, 1)

		accordion.select(index: 7)
		XCTAssertNil(accordion.selectedId)
		accordion.hover(id: "unknown")
		XCTAssertNil(accordion.hoveredId)
	}

	func testTheModelKeepsTheNotesStepsAndSelectionAcrossAReload() throws {
		let model = try story()
		XCTAssertEqual(model.accordion.steps.map(\.taskId), ["task-2", "task-3", "task-4", "task-5"])
		XCTAssertEqual(model.accordion.steps.map(\.depth), [0, 1, 1, 0])
		XCTAssertEqual(model.accordion.initialScrollId, "task-2")
		model.selectStep(index: 3)
		model.reload()
		XCTAssertEqual(model.accordion.selectedId, "task-5")
	}

	// MARK: Panel tabs and keys

	func testTabsStepAndPickByPlace() throws {
		let model = try story()
		XCTAssertEqual(model.panelTab, .hours)
		model.stepTab(by: 1)
		XCTAssertEqual(model.panelTab, .reports)
		model.stepTab(by: 1)
		model.stepTab(by: 1)
		XCTAssertEqual(model.panelTab, .hours, "the rail wraps")
		model.stepTab(by: -1)
		XCTAssertEqual(model.panelTab, .claudeCode)
		model.showTab(place: 2)
		XCTAssertEqual(model.panelTab, .reports)
		model.showTab(place: 9)
		XCTAssertEqual(model.panelTab, .reports, "no tab at that place")
		XCTAssertEqual(StoryPanelTab.allCases.filter(\.isPlanned), [.claudeCode])
		model.open(number: "WEB-157")
		XCTAssertEqual(model.panelTab, .hours, "a story opens at Hours")
	}

	private func keymap() throws -> Keymap {
		try Keymap.parse(String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)).get()
	}

	func testBracketsAndDigitsSwitchTheStoryPanelOnlyWhileItIsFocused() throws {
		let vim = KeyDispatcher(keymap: try keymap(), scheme: .vim)
		XCTAssertEqual(vim.handle(KeyStroke(key: "]"), in: .storyPanel), .action(.nextStoryPanelTab))
		XCTAssertEqual(vim.handle(KeyStroke(key: "["), in: .storyPanel), .action(.previousStoryPanelTab))
		XCTAssertEqual(vim.handle(KeyStroke(key: "2"), in: .storyPanel), .action(.showStoryPanelTab(2)))
		XCTAssertEqual(vim.handle(KeyStroke(key: "]"), in: .today), .action(.nextSidePanelTab), "elsewhere `]` stays the side panel's")
		XCTAssertEqual(vim.handle(KeyStroke(key: "2"), in: .today), .unbound)
		XCTAssertEqual(vim.handle(KeyStroke(key: "2"), in: .sidePanel), .action(.showSidePanelTab(2)))

		let standard = KeyDispatcher(keymap: try keymap(), scheme: .standard)
		XCTAssertEqual(standard.handle(KeyStroke(key: "tab", modifiers: .control), in: .storyPanel), .action(.nextStoryPanelTab))
		XCTAssertEqual(standard.handle(KeyStroke(key: "1", modifiers: .option), in: .storyPanel), .action(.showStoryPanelTab(1)))
		XCTAssertEqual(standard.handle(KeyStroke(key: "1", modifiers: .option), in: .today), .action(.showSidePanelTab(1)))
		XCTAssertNotNil(standard.title(for: .nextStoryPanelTab), "every story panel action has a label")
	}

	func testOnlyTheStoryPanelMayShadowAKeyBoundEverywhere() {
		let shadow = """
		version: 1
		bindings:
		  - action: side-panel.next
		    label: "Next"
		    vim: "]"
		    standard: ""
		  - action: story-panel.next
		    label: "Next story tab"
		    pane: "story-panel"
		    vim: "]"
		    standard: ""
		"""
		XCTAssertNoThrow(try Keymap.parse(shadow).get())
		let twice = shadow + """

		  - action: story-panel.previous
		    label: "Previous story tab"
		    pane: "story-panel"
		    vim: "]"
		    standard: ""
		"""
		XCTAssertThrowsError(try Keymap.parse(twice).get())
		let home = shadow.replacingOccurrences(of: "pane: \"story-panel\"", with: "pane: \"side-panel\"")
		XCTAssertThrowsError(try Keymap.parse(home).get(), "inside Home a pane key bound everywhere is still refused")
	}

	func testTheStoryScreenFocusStepsBetweenTheNoteAndItsPanel() {
		let focus = PaneFocusModel(focusedPane: .sidePanel, scheme: .standard)
		focus.setPaneOrder(Pane.storyOrder)
		XCTAssertEqual(focus.focusedPane, .today, "a pane that left the screen gives up focus")
		focus.apply(.moveRight)
		XCTAssertEqual(focus.focusedPane, .storyPanel)
		focus.apply(.moveRight)
		XCTAssertEqual(focus.focusedPane, .storyPanel, "no pane right of the panel")
		focus.apply(.moveLeft)
		XCTAssertEqual(focus.focusedPane, .today)
		focus.apply(.showStoryPanelTab(1))
		XCTAssertEqual(focus.focusedPane, .storyPanel)
		focus.setPaneOrder(Pane.homeOrder)
		XCTAssertEqual(focus.focusedPane, .today)
		focus.apply(.moveRight)
		XCTAssertEqual(focus.focusedPane, .inbox, "the Home window steps through its own four")
	}

	// MARK: Writes

	func testTickingABodyCheckboxChangesOneVaultLine() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let note = vault.appending(path: "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md")
		let before = try String(contentsOf: note, encoding: .utf8)
		let model = try story(at: vault.path)

		model.toggle(taskId: "task-1", isDone: true)

		XCTAssertNil(model.writeMessage)
		let after = try String(contentsOf: note, encoding: .utf8)
		let beforeLines = before.components(separatedBy: "\n")
		let afterLines = after.components(separatedBy: "\n")
		XCTAssertEqual(beforeLines.count, afterLines.count)
		let changed = zip(beforeLines, afterLines).filter { $0 != $1 }
		XCTAssertEqual(changed.count, 1, "\(changed)")
		XCTAssertTrue(changed.first?.1.hasPrefix("- [ ] Extract the address form") ?? false, "\(changed)")
		var checkboxes: [StoryCheckbox] = []
		StoryBlock.visit(model.content?.blocks ?? []) { block in
			if case let .listItem(_, _, checkbox?) = block.kind { checkboxes.append(checkbox) }
		}
		XCTAssertEqual(checkboxes.first?.state, .open, "the screen reads the note again")
	}

	func testStepsTickScheduleAndReEstimateThroughTheirLineOnly() throws {
		let writer = RecordingStoryLines()
		let model = try story(lines: writer)

		model.toggle(taskId: "task-2", isDone: false)
		model.schedule(taskId: "task-2", on: model.scheduleChoices.tomorrow)
		model.estimate(taskId: "task-5", to: "45m")
		model.estimate(taskId: "task-5", to: nil)
		model.toggle(taskId: "task-1", isDone: true)

		XCTAssertEqual(writer.edits.map { $0.taskId }, ["task-2", "task-2", "task-5", "task-5", "task-1"])
		XCTAssertEqual(writer.edits.map { $0.edit }, [.tick(on: "2026-10-22"), .schedule("2026-10-23"), .estimate("45m"), .estimate(nil), .reopen])
	}

	func testARefusedWriteSaysWhy() throws {
		let writer = RecordingStoryLines()
		writer.refusal = "The line changed in the note"
		let model = try story(lines: writer)
		model.toggle(taskId: "task-2", isDone: false)
		XCTAssertEqual(model.writeMessage, "The line changed in the note")
	}

	/// No code path of the story screen writes prose: its edit type has no
	/// text case, none maps to the core's text edit, and its sources call no
	/// text or file write.
	func testNoCodePathWritesProse() throws {
		let edits: [StoryLineEdit] = [.tick(on: "2026-10-22"), .reopen, .schedule("2026-10-23"), .schedule(nil), .estimate("1h"), .estimate(nil)]
		for edit in edits {
			if case .text = CoreStoryLines.input(for: edit) {
				XCTFail("\(edit) maps to a text edit")
			}
		}

		let apple = TestFixtures.repoRoot.appending(path: "apps/apple")
		let folders = [
			apple.appending(path: "VaultdayCore/Sources/VaultdayCore/Story"),
			apple.appending(path: "Vaultday/Views/Story"),
		]
		let forbidden = [".text(", "TaskEditInput.text", "changeText", "setText", "restoreTaskLine", "appendLog", "write(to:", "writeAtomic", "FileManager", "createFile", "addTask", "saveQuickAdd"]
		var checked = 0
		for folder in folders {
			let files = try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil).filter { $0.pathExtension == "swift" }
			XCTAssertFalse(files.isEmpty, folder.path)
			for file in files {
				let source = try String(contentsOf: file, encoding: .utf8)
				for word in forbidden {
					XCTAssertFalse(source.contains(word), "\(file.lastPathComponent) mentions \(word)")
				}
				checked += 1
			}
		}
		XCTAssertGreaterThanOrEqual(checked, 6)
	}
}

/// Records the story screen's line edits instead of writing them.
@MainActor
private final class RecordingStoryLines: StoryLineWriting {
	var edits: [(taskId: String, edit: StoryLineEdit)] = []
	var refusal: String?

	func apply(_ edit: StoryLineEdit, to taskId: String) -> LineWrite {
		edits.append((taskId, edit))
		return refusal.map(LineWrite.refused) ?? .written(old: "", new: "")
	}
}
