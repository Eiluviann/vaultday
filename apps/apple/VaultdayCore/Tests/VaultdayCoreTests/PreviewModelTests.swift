import Foundation
import XCTest
@testable import VaultdayCore

/// The task preview and the peek against the mock vault: the story note
/// cut into read-only blocks, the subtasks, the waiting items and the activity.
@MainActor
final class PreviewModelTests: XCTestCase {
	private static let today: () -> Date = { DayText.date(from: "2026-10-22") ?? Date() }

	private func preview(_ taskId: String) throws -> TaskPreviewContent {
		let model = PreviewModel(now: Self.today)
		model.attach(workspacePath: TestFixtures.mockVault.path)
		model.open(taskId: taskId)
		return try XCTUnwrap(model.content, model.errorMessage ?? "no content")
	}

	func testThePreviewCutsTheStoryNoteIntoSectionsWithoutTheTaskSections() throws {
		let content = try preview("task-2")

		XCTAssertEqual(content.headerLine, "task-2 · WEB-142 · Checkout redesign")
		XCTAssertEqual(content.storyStatusLabel, "In progress")
		XCTAssertEqual(content.sectionTitle, "Next Steps")
		XCTAssertEqual(content.estimateLabel, "2h")
		XCTAssertEqual(content.scheduledLabel, "today")
		let headings = content.blocks.compactMap { block -> String? in
			if case let .heading(_, text) = block { return text }
			return nil
		}
		XCTAssertEqual(headings, ["Background", "Implementation", "Notes"])
		XCTAssertFalse("\(content.blocks)".contains("Wire the payment form"), "the task sections are drawn as subtasks and waiting items")
		XCTAssertFalse("\(content.blocks)".contains("Final copy"))
		XCTAssertFalse("\(content.blocks)".contains("ai:end"), "provenance markers are hidden")
		XCTAssertFalse("\(content.blocks)".contains("type: story"), "no frontmatter")
		XCTAssertTrue(content.blocks.contains(.paragraph("Replace the three-step checkout with a single page. Ticket: [WEB-142](https://tracker.example.test/browse/WEB-142).")))
		XCTAssertTrue(content.blocks.contains(.paragraph("Render the address and payment forms side by side; validate on blur.")))
		XCTAssertTrue(content.blocks.contains(.bullet(depth: 0, text: "Extract the address form into its own component", state: .done)))
		XCTAssertTrue(content.blocks.contains(.bullet(depth: 0, text: "2026-10-21 Decided to keep the coupon field above the fold.", state: nil)))
	}

	func testSubtasksWaitingItemsAndActivityComeWithTheTask() throws {
		let content = try preview("task-2")

		XCTAssertEqual(content.subtasks.map(\.taskId), ["task-3", "task-4"])
		XCTAssertEqual(content.subtasks.map(\.state), [.open, .inProgress])
		XCTAssertEqual(content.subtasks.map(\.estimateLabel), [nil, "45m"])
		XCTAssertEqual(content.waiting.map(\.taskId), ["task-6"])
		XCTAssertEqual(content.waiting.first?.detail, "Dana · 12d")
		XCTAssertEqual(content.activity.first?.whenLabel, "09:30", "a line of today shows its time")
		XCTAssertEqual(content.activity.first?.kind, "session")
		XCTAssertTrue(content.activity.contains { $0.whenLabel == "21 Oct 12:30" && $0.kind == "blocker" })
		XCTAssertEqual(content.activity.count, 6, "the last six lines about the story")
	}

	func testTheObsidianLinkOpensTheNoteHoldingTheTask() throws {
		let content = try preview("task-2")
		let link = try XCTUnwrap(content.obsidianLink)

		XCTAssertEqual(link.scheme, "obsidian")
		let query = URLComponents(url: link, resolvingAgainstBaseURL: false)?.queryItems ?? []
		XCTAssertEqual(query.first { $0.name == "vault" }?.value, "mock-vault")
		XCTAssertEqual(query.first { $0.name == "file" }?.value, "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign")
	}

	func testAPendingInputTaskNamesItsSectionAndAnUnknownIdIsAnError() throws {
		let content = try preview("task-9")
		XCTAssertEqual(content.sectionTitle, "Pending Input")
		XCTAssertEqual(content.assignedTo, "Lee")

		let model = PreviewModel(now: Self.today)
		model.attach(workspacePath: TestFixtures.mockVault.path)
		model.open(taskId: "task-999")
		XCTAssertNil(model.content)
		XCTAssertTrue(model.errorMessage?.contains("task-999") == true, model.errorMessage ?? "nil")
		model.open(taskId: "task-2")
		XCTAssertTrue(model.isOpen)
		model.close()
		XCTAssertFalse(model.isOpen)
	}

	func testFullStoryOpensTheStoryScreenAndClaudeCodeIsNotHandledYet() {
		let model = PreviewModel(now: Self.today)
		XCTAssertEqual(model.claudeCodeLink(taskId: "task-2")?.absoluteString, "vaultday://claude-code/task/task-2")
		XCTAssertEqual(model.fullStoryLink(storyNumber: "WEB-142")?.absoluteString, "vaultday://story/WEB-142")
		XCTAssertEqual(
			model.fullStoryLink(storyNumber: "WEB-142").flatMap(VaultdayLink.init),
			.story(number: "WEB-142"),
		)
		XCTAssertEqual(
			model.claudeCodeLink(taskId: "task-2").flatMap(VaultdayLink.init),
			.notHandledYet(host: "claude-code", path: "/task/task-2"),
		)
	}

	func testWikilinksShowTheirTextAndInlineMarkdownRenders() {
		XCTAssertEqual(PreviewText.unwrapWikilinks("See [[Brand kit]]."), "See Brand kit.")
		XCTAssertEqual(PreviewText.unwrapWikilinks("See [[Brand kit|the kit]] and [[Plan#Phase 2]]."), "See the kit and Plan.")
		XCTAssertEqual(PreviewText.unwrapWikilinks("no links [ here"), "no links [ here")
		XCTAssertEqual(String(PreviewText.attributed("a **bold** [[X]]").characters), "a bold X")
	}

	// MARK: Peek

	func testAPeekOfATaskShowsTheRecordNotTheNote() throws {
		let model = PeekModel(now: Self.today)
		model.attach(workspacePath: TestFixtures.mockVault.path)

		model.show(.task("task-6"))

		let content = try XCTUnwrap(model.content, model.errorMessage ?? "no content")
		XCTAssertEqual(content.chips, ["task-6", "WEB-142"])
		XCTAssertEqual(content.subtitle, "Checkout redesign · in progress")
		XCTAssertEqual(content.badge, "waiting on Dana · 12d")
		XCTAssertEqual(content.title, "Final copy for the order summary")
		XCTAssertEqual(content.fields.map(\.label), ["Section", "Created", "Assigned to"])
		XCTAssertEqual(content.fields.first?.value, "## Pending Input")
		XCTAssertEqual(content.fields[1].value, "10 Oct")
		XCTAssertEqual(content.taskId, "task-6")
		XCTAssertNotNil(content.obsidianLink)
		XCTAssertLessThanOrEqual(content.activity.count, 3)
	}

	func testAPeekOfAStoryAndALogLineAndACalendarBlock() throws {
		let model = PeekModel(now: Self.today)
		model.attach(workspacePath: TestFixtures.mockVault.path)

		model.show(.story("WEB-142"))
		var content = try XCTUnwrap(model.content)
		XCTAssertEqual(content.title, "Checkout redesign")
		XCTAssertEqual(content.chips, ["WEB-142"])
		XCTAssertEqual(content.fields.map(\.label), ["Status", "Priority", "Due", "Estimate", "Waiting on"])
		XCTAssertEqual(content.fields.last?.value, "Dana")
		XCTAssertEqual(content.excerpt, "Replace the three-step checkout with a single page. Ticket: [WEB-142](https://tracker.example.test/browse/WEB-142).")

		let line = DayLogLine(id: "x", time: "11:15", kind: "done", text: "Testing steps drafted", story: nil, task: "task-3", agent: "claude-fable-5-1")
		model.show(.logLine(line))
		content = try XCTUnwrap(model.content)
		XCTAssertEqual(content.title, "Testing steps drafted")
		XCTAssertEqual(content.fields.last?.value, "claude-fable-5-1")
		XCTAssertEqual(content.taskId, "task-3")

		model.show(.block(title: "Design review", start: "11:00", end: "12:00", source: "Work calendar", taskId: nil))
		content = try XCTUnwrap(model.content)
		XCTAssertEqual(content.subtitle, "Work calendar")
		XCTAssertEqual(content.fields.map(\.value), ["11:00", "12:00"])
	}

	func testPeekTogglesAndDismissesAndRefusesAnUnknownTask() {
		let model = PeekModel(now: Self.today)
		model.attach(workspacePath: TestFixtures.mockVault.path)

		model.toggle(.task("task-2"))
		XCTAssertEqual(model.subject, .task("task-2"))
		model.toggle(.task("task-2"))
		XCTAssertNil(model.subject)
		model.show(.task("task-2"))
		model.dismiss()
		XCTAssertNil(model.content)
		model.show(.task("task-999"))
		XCTAssertNil(model.subject)
		XCTAssertNotNil(model.errorMessage)
	}
}
