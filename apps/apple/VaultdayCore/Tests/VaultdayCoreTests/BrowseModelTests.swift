import Foundation
import XCTest
@testable import VaultdayCore

/// Browse against the synthetic mock vault: the core parses and runs the
/// query line; the model groups, folds, remembers presets and keeps the
/// selectable lines.
@MainActor
final class BrowseModelTests: XCTestCase {
	private static let mockVault = TestFixtures.mockVault.path
	/// Thursday 22 October 2026, the fixture's "today".
	private static let today: () -> Date = { DayText.date(from: "2026-10-22") ?? Date() }

	private func model() -> BrowseModel {
		let model = BrowseModel(now: Self.today)
		model.load(workspacePath: Self.mockVault)
		return model
	}

	private func ids(_ model: BrowseModel) -> [String] {
		model.groups.flatMap(\.sections).flatMap(\.rows).map(\.id)
	}

	func testTheFullTableGroupsByStoryThenSection() {
		let model = model()

		XCTAssertNil(model.errorMessage)
		XCTAssertEqual(model.groups.map(\.storyNumber), ["WEB-142", "WEB-157", "ACME-9"])
		XCTAssertEqual(model.groups.map(\.title), ["Checkout redesign", "Newsletter signup", "Onboarding emails"])
		XCTAssertEqual(model.groups[0].sections.map(\.title), ["Next Steps", "Pending Input"])
		XCTAssertEqual(model.groups[0].sections.map { $0.rows.map(\.id) }, [["task-2", "task-5"], ["task-6"]])
		XCTAssertEqual(model.groups[2].sections.map(\.title), ["Next Steps"])
		XCTAssertEqual(model.groups.map(\.countLabel), ["3 open", "2 open", "1 open"])
		XCTAssertEqual(model.taskCount, 6)
		XCTAssertEqual(model.summary, "Mock Studio · 6 open across 3 stories")
	}

	func testRowsCarryTheColumnsAsTheTableWritesThem() throws {
		let model = model()
		let rows = model.groups.flatMap(\.sections).flatMap(\.rows)

		let wire = try XCTUnwrap(rows.first { $0.id == "task-2" })
		XCTAssertEqual(wire.scheduledLabel, "today")
		XCTAssertEqual(wire.estimateLabel, "2h")
		XCTAssertEqual(wire.storyNumber, "WEB-142")
		XCTAssertNil(wire.dueLabel)
		let test = try XCTUnwrap(rows.first { $0.id == "task-5" })
		XCTAssertEqual(test.dueLabel, "Oct 28")
		XCTAssertFalse(test.isOverdue)
		let copy = try XCTUnwrap(rows.first { $0.id == "task-6" })
		XCTAssertEqual(copy.waitingOn, "Dana")
		XCTAssertEqual(copy.waitingDays, 12)
	}

	func testTypedFiltersNarrowTheTable() {
		let model = model()

		model.queryText = "waiting:any"
		model.submitQuery()
		XCTAssertEqual(ids(model), ["task-6", "task-9"])
		XCTAssertEqual(model.appliedQuery, "waiting:any")

		model.queryText = "story:157 section:next"
		model.submitQuery()
		XCTAssertEqual(ids(model), ["task-8"])

		model.queryText = "scheduled:today estimate:>1h"
		model.submitQuery()
		XCTAssertEqual(ids(model), ["task-2"])

		model.queryText = "due:>today"
		model.submitQuery()
		XCTAssertEqual(ids(model), ["task-5"])

		model.queryText = "status:done"
		model.submitQuery()
		XCTAssertEqual(ids(model), ["task-7", "task-13"])

		model.queryText = ""
		model.submitQuery()
		XCTAssertEqual(model.taskCount, 6)
	}

	func testAQueryTheCoreRefusesKeepsTheTableAndSaysWhy() {
		let model = model()
		model.queryText = "story:157"
		model.submitQuery()
		let before = ids(model)

		model.queryText = "due:soonish"
		model.submitQuery()

		XCTAssertEqual(ids(model), before)
		XCTAssertEqual(model.appliedQuery, "story:157")
		XCTAssertTrue(model.queryMessage?.hasPrefix("due:soonish") == true, model.queryMessage ?? "nil")

		model.queryText = "stroy:157"
		model.submitQuery()
		XCTAssertTrue(model.queryMessage?.contains("not a filter") == true, model.queryMessage ?? "nil")

		model.queryText = "story:157"
		model.submitQuery()
		XCTAssertNil(model.queryMessage)
	}

	func testBacklogIsAPresetOfTheCore() throws {
		let model = model()
		let backlog = try XCTUnwrap(model.presets.first { $0.name == "Backlog" })

		model.apply(backlog)

		XCTAssertTrue(backlog.isBuiltIn)
		XCTAssertEqual(model.queryText, "status:open section:next scheduled:none")
		XCTAssertEqual(ids(model), ["task-5", "task-8", "task-12"])
		XCTAssertEqual(model.activePreset?.name, "Backlog")
		XCTAssertEqual(model.presets.map(\.name).prefix(2), ["All open", "Backlog"])
	}

	func testSavedPresetsAreKeptPerWorkspaceOnThisMacAndCanBeRemoved() throws {
		let file = try TestFixtures.temporaryFolder(self, "presets").appending(path: "browse.json")
		let model = BrowseModel(now: Self.today)
		model.load(workspacePath: Self.mockVault, presetFile: file)
		model.queryText = "waiting:dana"
		model.submitQuery()

		XCTAssertTrue(model.savePreset(named: "  Dana  "))
		XCTAssertFalse(model.savePreset(named: "dana"), "a name is used once")
		XCTAssertFalse(model.savePreset(named: "Backlog"), "a built-in name is taken")
		XCTAssertFalse(model.savePreset(named: "   "))
		XCTAssertEqual(model.activePreset?.name, "Dana")

		let reopened = BrowseModel(now: Self.today)
		reopened.load(workspacePath: Self.mockVault, presetFile: file)
		XCTAssertEqual(reopened.presets.map(\.name), ["All open", "Backlog", "Chasers", "Active stories", "Due this week", "Braindump", "Dana"])
		XCTAssertEqual(reopened.count(of: try XCTUnwrap(reopened.presets.last)), 1, "a saved preset gets its count too")
		let saved = try XCTUnwrap(reopened.presets.last)
		XCTAssertFalse(saved.isBuiltIn)
		reopened.apply(saved)
		XCTAssertEqual(ids(reopened), ["task-6"])

		reopened.removePreset(saved)
		reopened.removePreset(try XCTUnwrap(reopened.presets.first))
		XCTAssertEqual(reopened.presets.count, 6, "built-in presets stay")
		let again = BrowseModel(now: Self.today)
		again.load(workspacePath: Self.mockVault, presetFile: file)
		XCTAssertEqual(again.presets.count, 6)
	}

	func testEveryPresetChipCarriesItsCountFromTheSameRead() {
		let model = model()

		let counts = model.presets.map { "\($0.name) \(model.count(of: $0).map(String.init) ?? "-")" }

		XCTAssertEqual(counts, ["All open 6", "Backlog 3", "Chasers 2", "Active stories 3", "Due this week 0", "Braindump 1"])
	}

	func testTheBacklogTabSharesBrowsesReadAndARefreshReadsOnceForBoth() throws {
		let copy = try TestFixtures.copyOfMockVault(self)
		let browse = BrowseModel(now: Self.today)
		browse.load(workspacePath: copy.path)
		let backlogLine = try XCTUnwrap(browse.presets.first { $0.name == "Backlog" }).query
		let backlog = BrowseModel(now: Self.today)
		backlog.load(sharingReadOf: browse, query: backlogLine)
		XCTAssertEqual(ids(backlog), ["task-5", "task-8", "task-12"])
		XCTAssertEqual(backlog.activePreset?.name, "Backlog")
		XCTAssertEqual(backlog.summary, browse.summary)

		// A query runs on the read already made: a note removed since is still listed.
		let note = copy.appending(path: "Projects/Acme portal/ACME-9 Onboarding emails/ACME-9 Onboarding emails.md")
		try FileManager.default.removeItem(at: note)
		browse.queryText = "story:ACME-9"
		browse.submitQuery()
		XCTAssertEqual(ids(browse), ["task-12"])

		// One refresh reads the vault again for both models.
		BrowseModel.refresh([browse, backlog])
		XCTAssertEqual(ids(browse), [])
		XCTAssertEqual(ids(backlog), ["task-5", "task-8"])
		XCTAssertEqual(browse.count(of: try XCTUnwrap(browse.presets.first)), 5)
	}

	func testGroupFiltersShapeTheTable() {
		let model = model()

		model.queryText = "group:section"
		model.submitQuery()
		XCTAssertEqual(model.groups.map(\.title), ["All stories"])
		XCTAssertNil(model.groups[0].storyNumber)
		XCTAssertEqual(model.groups[0].sections.map(\.title), ["Next Steps", "Pending Input"])

		model.queryText = "group:story story:142"
		model.submitQuery()
		XCTAssertEqual(model.groups.map(\.storyNumber), ["WEB-142"])
		XCTAssertEqual(model.groups[0].sections.map(\.title), [nil])
		XCTAssertEqual(ids(model), ["task-2", "task-5", "task-6"])

		model.queryText = "group:task"
		model.submitQuery()
		XCTAssertNotNil(model.queryMessage)
	}

	func testAQueryWithAnErrorIsNotSavedAsAPreset() {
		let model = model()
		model.queryText = "due:soonish"
		model.submitQuery()

		XCTAssertFalse(model.savePreset(named: "Broken"))
	}

	func testFoldsHideRowsButKeepTheHeaderAndTheCount() throws {
		let model = model()
		let checkout = model.groups[0]
		XCTAssertEqual(model.items.count, 9, "three headers and six rows")

		model.toggleFold(checkout)

		XCTAssertTrue(model.isFolded(checkout))
		XCTAssertEqual(model.items.count, 3 + 3, "the folded group keeps its header and drops its three rows")
		XCTAssertEqual(model.taskCount, 6, "folding does not change what matches")
		XCTAssertEqual(model.items.first, .header(checkout))

		model.toggleFold(checkout)
		XCTAssertFalse(model.isFolded(checkout))
		XCTAssertEqual(model.items.count, 9)
	}

	func testTheSelectableLinesWalkHeadersAndRowsAndJumpBetweenGroups() throws {
		let model = model()
		// header, task-2, task-5, task-6, header, task-8, task-9, header, task-12
		XCTAssertEqual(model.items.map(\.id), [
			"story:WEB-142", "task:task-2", "task:task-5", "task:task-6",
			"story:WEB-157", "task:task-8", "task:task-9",
			"story:ACME-9", "task:task-12",
		])

		XCTAssertEqual(model.nextGroupStart(after: 0), 4)
		XCTAssertEqual(model.nextGroupStart(after: 2), 4)
		XCTAssertEqual(model.nextGroupStart(after: 4), 7)
		XCTAssertEqual(model.nextGroupStart(after: 7), 8, "past the last group it stays on the last line")
		XCTAssertEqual(model.previousGroupStart(before: 6), 4)
		XCTAssertEqual(model.previousGroupStart(before: 4), 0)
		XCTAssertEqual(model.previousGroupStart(before: 0), 0)

		XCTAssertEqual(model.group(at: 2)?.storyNumber, "WEB-142")
		XCTAssertEqual(model.group(at: 4)?.storyNumber, "WEB-157")
		XCTAssertNil(model.group(at: 99))
		XCTAssertEqual(model.index(ofTask: "task-9"), 6)

		model.toggleFold(model.groups[0])
		XCTAssertNil(model.index(ofTask: "task-2"))
		XCTAssertEqual(model.nextGroupStart(after: 0), 1)
	}

	func testAFolderWithoutAManifestReportsAnError() {
		let model = BrowseModel(now: Self.today)
		model.load(workspacePath: TestFixtures.repoRoot.path)

		XCTAssertEqual(model.groups, [])
		XCTAssertTrue(model.errorMessage?.contains("not a workspace") == true, model.errorMessage ?? "nil")
	}

	func testDatesReadTheWayTheTableWritesThem() {
		XCTAssertEqual(BrowseDateText.scheduled("2026-10-22", today: "2026-10-22"), "today")
		XCTAssertEqual(BrowseDateText.scheduled("2026-10-23", today: "2026-10-22"), "tomorrow")
		XCTAssertEqual(BrowseDateText.scheduled("2026-10-21", today: "2026-10-22"), "yesterday")
		XCTAssertEqual(BrowseDateText.scheduled("2026-10-24", today: "2026-10-22"), "Sat")
		XCTAssertEqual(BrowseDateText.scheduled("2026-11-03", today: "2026-10-22"), "Nov 3")
		XCTAssertEqual(BrowseDateText.due("2026-10-20"), "Oct 20")
	}
}
