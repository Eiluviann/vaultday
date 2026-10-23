import Foundation
import Observation

/// The Browse view: every task of the workspace's stories as a table
/// grouped by story, then section. The query line is parsed and run by the
/// core; this model keeps the result, the folds, the presets and the
/// selectable lines the keyboard walks. The vault is read once per refresh
/// into a snapshot; the query, the summary and the preset counts all run on
/// it, and a model can share another's snapshot (the Backlog tab).
@MainActor
@Observable
public final class BrowseModel {
	public private(set) var groups: [BrowseStoryGroup] = []
	/// How many rows match the query, folded or not.
	public private(set) var taskCount = 0
	/// What the core said about the query line when it was not understood.
	public private(set) var queryMessage: String?
	/// A problem reading the vault, apart from the query.
	public private(set) var errorMessage: String?
	/// The line in the `/` field.
	public var queryText = ""
	public private(set) var appliedQuery = ""
	public private(set) var presets: [BrowsePreset] = []
	/// The group ids folded away; kept while the query changes.
	public private(set) var foldedGroupIds: Set<String> = []

	/// How many tasks each preset's line matches, by preset name; a line the core refuses has none.
	public private(set) var presetCounts: [String: Int] = [:]

	private var vault: WorkspaceHandle?
	/// The vault as last read; queries run on it without reading again.
	private var snapshot: BrowseSnapshot?
	private var store = BrowsePresetStore(file: nil)
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
		presets = Self.builtInPresets()
	}

	/// "Work · 41 open across 12 stories", from the unfiltered counts.
	public private(set) var summary = ""

	// MARK: Loading

	/// Opens the workspace's tasks. `presetFile` is where this workspace's
	/// saved presets live on this Mac.
	public func load(from workspace: WorkspaceModel, presetFile: URL? = nil) {
		guard let vault = workspace.vault else {
			clear()
			return
		}
		attach(vault, name: workspace.name, presetFile: presetFile)
	}

	/// Opens the workspace at `path` without a grant (tests, and hosts outside the sandbox).
	public func load(workspacePath path: String, presetFile: URL? = nil) {
		do {
			let vault = try openWorkspace(path: path)
			attach(vault, name: vault.name(), presetFile: presetFile)
		} catch {
			clear()
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Opens the workspace `other` has open on the snapshot it has read, with
	/// `query` applied: the Backlog tab beside Browse, without a read of its own.
	public func load(sharingReadOf other: BrowseModel, query: String) {
		guard let vault = other.vault, let snapshot = other.snapshot else {
			clear()
			return
		}
		self.vault = vault
		workspaceName = other.workspaceName
		queryText = query
		appliedQuery = query
		show(snapshot)
	}

	/// Reads the vault again with the query line applied now.
	public func refresh() {
		Self.refresh([self])
	}

	/// Reads the vault once and reruns every model's query, summary and
	/// preset counts on that one read. Models on another workspace read their own.
	public static func refresh(_ models: [BrowseModel]) {
		var reads: [(vault: WorkspaceHandle, snapshot: BrowseSnapshot?)] = []
		for model in models {
			guard let vault = model.vault else { continue }
			if let read = reads.first(where: { $0.vault === vault }) {
				if let snapshot = read.snapshot { model.show(snapshot) }
				continue
			}
			let snapshot = model.read(vault)
			reads.append((vault, snapshot))
			if let snapshot { model.show(snapshot) }
		}
	}

	/// Applies the line in the `/` field to the vault as last read.
	public func submitQuery() {
		guard let snapshot else { return }
		run(snapshot, query: queryText)
	}

	/// How many tasks `preset` matches, for its chip.
	public func count(of preset: BrowsePreset) -> Int? {
		presetCounts[preset.name]
	}

	/// Shows a preset: its line fills the field and runs.
	public func apply(_ preset: BrowsePreset) {
		queryText = preset.query
		submitQuery()
	}

	/// The preset whose line is the one applied.
	public var activePreset: BrowsePreset? {
		presets.first { $0.query == appliedQuery }
	}

	/// Saves the applied query line under `name`. `false` when the name is
	/// empty or taken, or the line is not one the core understands.
	@discardableResult
	public func savePreset(named name: String) -> Bool {
		let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
		guard !name.isEmpty, queryMessage == nil, !presets.contains(where: { $0.name.caseInsensitiveCompare(name) == .orderedSame })
		else { return false }
		presets.append(BrowsePreset(name: name, query: appliedQuery))
		store.save(presets)
		if let snapshot { updatePresetCounts(snapshot) }
		return true
	}

	/// Removes a preset the user saved; the built-in ones stay.
	public func removePreset(_ preset: BrowsePreset) {
		guard !preset.isBuiltIn else { return }
		presets.removeAll { $0.name == preset.name }
		store.save(presets)
	}

	// MARK: Folds

	public func isFolded(_ group: BrowseStoryGroup) -> Bool {
		foldedGroupIds.contains(group.id)
	}

	/// `za`: folds the group, or opens it again.
	public func toggleFold(_ group: BrowseStoryGroup) {
		if !foldedGroupIds.insert(group.id).inserted {
			foldedGroupIds.remove(group.id)
		}
	}

	/// The group the selectable line at `index` belongs to.
	public func group(at index: Int) -> BrowseStoryGroup? {
		guard items.indices.contains(index) else { return nil }
		switch items[index] {
		case let .header(group): return group
		case let .row(row): return groups.first { $0.sections.contains { $0.rows.contains(row) } }
		}
	}

	// MARK: Selectable lines

	/// Every line the keyboard can select: a header per group, then its rows unless folded.
	public var items: [BrowseItem] {
		groups.flatMap { group -> [BrowseItem] in
			let rows = foldedGroupIds.contains(group.id) ? [] : group.sections.flatMap(\.rows).map(BrowseItem.row)
			return [.header(group)] + rows
		}
	}

	/// The index of the first line of the next group after `index` (`}`), else the last line.
	public func nextGroupStart(after index: Int) -> Int {
		let starts = groupStarts
		return starts.first { $0 > index } ?? max(0, items.count - 1)
	}

	/// The index of the header of the group `index` sits in, or of the one before when `index` is a header (`{`).
	public func previousGroupStart(before index: Int) -> Int {
		groupStarts.last { $0 < index } ?? 0
	}

	// MARK: Matches

	/// The words `n` and `N` look for: the applied query's free text and its
	/// `text:` values, lower case. Empty when there is no query or it only
	/// filters, and then `n` and `N` stay put.
	public var matchTerms: [String] { Self.matchTerms(in: appliedQuery) }

	/// The line `n` (`isForward`) or `N` goes to from `index`: the next or
	/// previous row whose text or id holds every match term, wrapping past
	/// the end; `nil` when no row matches or there is no text to match.
	public func matchIndex(from index: Int?, isForward: Bool) -> Int? {
		let terms = matchTerms
		guard !terms.isEmpty else { return nil }
		let matching = items.indices.filter { position in
			guard case let .row(row) = items[position] else { return false }
			let haystack = (row.text + " " + row.id).lowercased()
			return terms.allSatisfy(haystack.contains)
		}
		guard !matching.isEmpty else { return nil }
		guard let index else { return isForward ? matching.first : matching.last }
		if isForward {
			return matching.first { $0 > index } ?? matching.first
		}
		return matching.last { $0 < index } ?? matching.last
	}

	/// The free-text terms of a query line, as the core reads them: a term
	/// without a filter key is text, `text:` adds its value, quotes keep spaces.
	static func matchTerms(in query: String) -> [String] {
		splitTerms(query).compactMap { term in
			guard let colon = term.firstIndex(of: ":") else { return unquoted(term).lowercased() }
			let key = term[..<colon].lowercased()
			let value = unquoted(String(term[term.index(after: colon)...]))
			if key == "text" { return value.isEmpty ? nil : value.lowercased() }
			return filterKeys.contains(key) ? nil : unquoted(term).lowercased()
		}
		.filter { !$0.isEmpty }
	}

	/// The core's filter keys (`domain::browse`); any other `word:` is text.
	static let filterKeys: Set<String> = [
		"status", "story", "story-status", "section", "due", "scheduled", "created", "source",
		"estimate", "waiting", "assigned", "group", "sort",
	]

	private static func splitTerms(_ line: String) -> [String] {
		var terms: [String] = []
		var current = ""
		var isQuoted = false
		for character in line {
			if character == "\"" { isQuoted.toggle() }
			if character.isWhitespace, !isQuoted {
				if !current.isEmpty { terms.append(current) }
				current = ""
			} else {
				current.append(character)
			}
		}
		if !current.isEmpty { terms.append(current) }
		return terms
	}

	private static func unquoted(_ text: String) -> String {
		text.replacingOccurrences(of: "\"", with: "")
	}

	/// The line holding task `taskId`, if shown.
	public func index(ofTask taskId: String) -> Int? {
		items.firstIndex { item in
			if case let .row(row) = item { return row.id == taskId }
			return false
		}
	}

	private var groupStarts: [Int] {
		items.enumerated().compactMap { index, item in
			if case .header = item { return index }
			return nil
		}
	}

	// MARK: Private

	private func attach(_ vault: WorkspaceHandle, name: String, presetFile: URL?) {
		self.vault = vault
		store = BrowsePresetStore(file: presetFile)
		presets = Self.builtInPresets() + store.load().filter { saved in
			!Self.builtInPresets().contains { $0.name.caseInsensitiveCompare(saved.name) == .orderedSame }
		}
		workspaceName = name
		if appliedQuery.isEmpty { appliedQuery = queryText }
		if let snapshot = read(vault) { show(snapshot) }
	}

	private func clear() {
		vault = nil
		snapshot = nil
		presetCounts = [:]
		groups = []
		taskCount = 0
		summary = ""
		queryMessage = nil
		errorMessage = nil
	}

	/// Reads the vault into a snapshot; `nil`, with the message shown, when it cannot be read.
	private func read(_ vault: WorkspaceHandle) -> BrowseSnapshot? {
		do {
			return try browseSnapshot(workspace: vault, today: DayText.string(now()))
		} catch {
			errorMessage = coreErrorMessage(error)
			return nil
		}
	}

	/// Shows a fresh snapshot: the applied query, the summary and the preset counts.
	private func show(_ snapshot: BrowseSnapshot) {
		self.snapshot = snapshot
		run(snapshot, query: appliedQuery)
		updateSummary(snapshot)
		updatePresetCounts(snapshot)
	}

	private func run(_ snapshot: BrowseSnapshot, query: String) {
		do {
			let result = try snapshot.query(query: query)
			let today = DayText.string(now())
			groups = result.groups.map { Self.group($0, today: today) }
			taskCount = Int(result.taskCount)
			appliedQuery = query
			queryMessage = nil
			errorMessage = nil
		} catch let error as CoreError {
			// A query the core refuses keeps the table as it was; the message is shown under the field.
			if case .InvalidInput = error {
				queryMessage = coreErrorMessage(error)
			} else {
				errorMessage = coreErrorMessage(error)
			}
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	private var workspaceName = ""

	private func updateSummary(_ snapshot: BrowseSnapshot) {
		guard let open = try? snapshot.count(query: "status:open") else { return }
		let stories = Int(open.storyCount)
		summary = "\(workspaceName) · \(open.taskCount) open across \(stories) \(stories == 1 ? "story" : "stories")"
	}

	private func updatePresetCounts(_ snapshot: BrowseSnapshot) {
		presetCounts = Dictionary(uniqueKeysWithValues: presets.compactMap { preset in
			(try? snapshot.count(query: preset.query)).map { (preset.name, Int($0.taskCount)) }
		})
	}

	private static func builtInPresets() -> [BrowsePreset] {
		browsePresets().map { BrowsePreset(name: $0.name, query: $0.query, isBuiltIn: true) }
	}

	private static func group(_ view: BrowseGroupView, today: String) -> BrowseStoryGroup {
		let id = view.spansStories ? "all-stories" : view.storyNumber ?? view.storyTitle ?? "no-story"
		return BrowseStoryGroup(
			id: id,
			storyNumber: view.storyNumber,
			title: view.spansStories ? "All stories" : view.storyTitle ?? view.storyNumber ?? "No story",
			taskCount: Int(view.taskCount),
			sections: view.sections.map { section in
				let (suffix, title): (String, String?) = switch section.section {
				case .nextSteps: ("#next", "Next Steps")
				case .pendingInput: ("#pending", "Pending Input")
				case nil: ("#all", nil)
				}
				return BrowseSectionGroup(
					id: id + suffix,
					title: title,
					rows: section.rows.map { Self.row($0, today: today) },
				)
			},
		)
	}

	private static func row(_ view: BrowseRowView, today: String) -> BrowseRow {
		let state: BrowseRow.State = switch view.state {
		case .open: .open
		case .inProgress: .inProgress
		case .done: .done
		case .cancelled: .cancelled
		}
		return BrowseRow(
			id: view.id,
			text: view.text,
			state: state,
			waitingOn: view.waitingOn,
			waitingDays: view.waitingDays.map(Int.init),
			scheduledLabel: view.scheduled.map { BrowseDateText.scheduled($0, today: today) },
			dueLabel: view.due.map(BrowseDateText.due),
			isOverdue: state == .open || state == .inProgress ? (view.due.map { $0 < today } ?? false) : false,
			estimateMinutes: view.estimateMinutes.map(Int.init),
			storyNumber: view.storyNumber,
		)
	}
}
