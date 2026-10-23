import Foundation
import Observation

/// The story screen: the note rendered read-only the way Obsidian's Reading
/// view shows it, the Next Steps accordion, the Story block and the tabbed
/// panel (Hours, Reports, Claude Code). The only writes are a task line's
/// checkbox, scheduled day and estimate, through ``StoryLineWriting``; free
/// text goes through "Edit in Obsidian".
@MainActor
@Observable
public final class StoryModel {
	public private(set) var content: StoryContent?
	public private(set) var hours: StoryHours?
	public private(set) var errorMessage: String?
	/// Why the last line edit was refused.
	public private(set) var writeMessage: String?
	public private(set) var accordion = NextStepsAccordion(steps: [])
	public private(set) var panelTab: StoryPanelTab = .hours
	/// The section last pointed at, where Open in Obsidian opens the note.
	public private(set) var currentSection: String?
	/// The Hours tab's "Toggl · this week" slot, for the open story.
	public let togglWeek = TogglWeekModel()

	private var vault: WorkspaceHandle?
	private var state: WorkspaceStateHandle?
	private var lines: (any StoryLineWriting)?
	private var togglSource: (any TogglWeekSource)?
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	public var isOpen: Bool { content != nil }
	public var number: String? { content?.number }

	/// Points the model at the open workspace; an open story closes.
	public func attach(to workspace: WorkspaceModel) {
		vault = workspace.vault
		state = workspace.state
		lines = workspace.vault.map(CoreStoryLines.init)
		togglSource = workspace.togglWeekSource
		close()
	}

	/// Replaces where the Toggl week comes from, for tests.
	public func attachToggl(source: (any TogglWeekSource)?) {
		togglSource = source
		reload()
	}

	/// Opens the workspace at `path` without a grant (tests, and hosts
	/// outside the sandbox). `lines` replaces the core writer, for tests.
	public func attach(workspacePath path: String, lines: (any StoryLineWriting)? = nil) {
		vault = try? openWorkspace(path: path)
		state = nil
		togglSource = nil
		self.lines = lines ?? vault.map(CoreStoryLines.init)
		close()
	}

	/// Shows the story numbered `number`, at the Hours tab and the first open step.
	public func open(number: String) {
		panelTab = .hours
		currentSection = nil
		load(number: number)
		accordion.select(id: nil)
	}

	/// Reads the open story again, keeping the selection and the tab.
	public func reload() {
		guard let number else { return }
		load(number: number)
	}

	public func close() {
		content = nil
		hours = nil
		togglWeek.clear()
		errorMessage = nil
		writeMessage = nil
		accordion = NextStepsAccordion(steps: [])
		currentSection = nil
	}

	// MARK: Panel

	public func showTab(_ tab: StoryPanelTab) {
		panelTab = tab
	}

	/// `]` and `[`: the next and the previous tab.
	public func stepTab(by step: Int) {
		panelTab = panelTab.stepped(by: step)
	}

	/// A bare digit: the tab at that place on the rail.
	public func showTab(place: Int) {
		if let tab = StoryPanelTab.at(place: place) { panelTab = tab }
	}

	// MARK: Accordion

	public func selectStep(index: Int?) {
		accordion.select(index: index)
	}

	public func hoverStep(id: String?) {
		accordion.hover(id: id)
	}

	/// The pointer is over a section: Open in Obsidian opens the note there.
	public func pointAt(section: String?) {
		currentSection = section
	}

	// MARK: Links

	/// Open in Obsidian: the note at the section pointed at, else at Next
	/// Steps, else the note.
	public var obsidianLink: URL? {
		guard let content else { return nil }
		let links = content.sectionLinks
		return currentSection.flatMap { links[$0] } ?? links["Next Steps"] ?? content.obsidianLink
	}

	/// Edit in Obsidian at `section`, else the note.
	public func editLink(section: String?) -> URL? {
		section.flatMap { content?.sectionLinks[$0] } ?? content?.obsidianLink
	}

	/// Open in Claude Code. The scheme is registered but nothing listens yet.
	public var claudeCodeLink: URL? {
		number.flatMap { VaultdayLink.url(host: "claude-code", path: "/story/" + $0) }
	}

	// MARK: Line edits

	/// Ticks a task line done, or opens it again when it is done.
	public func toggle(taskId: String, isDone: Bool) {
		write(isDone ? .reopen : .tick(on: DayText.string(now())), to: taskId)
	}

	/// Schedules a step on `day` (`YYYY-MM-DD`), or removes the day.
	public func schedule(taskId: String, on day: String?) {
		write(.schedule(day), to: taskId)
	}

	/// Sets a step's estimate (`45m`, `2h`), or removes it.
	public func estimate(taskId: String, to estimate: String?) {
		write(.estimate(estimate), to: taskId)
	}

	/// The host's today and tomorrow, `YYYY-MM-DD`, for the schedule menu.
	public var scheduleChoices: (today: String, tomorrow: String) {
		let today = now()
		let tomorrow = Calendar(identifier: .gregorian).date(byAdding: .day, value: 1, to: today) ?? today
		return (DayText.string(today), DayText.string(tomorrow))
	}

	/// "today", "tomorrow", "Thu", "Oct 20".
	public func scheduledLabel(_ iso: String) -> String {
		BrowseDateText.scheduled(iso, today: DayText.string(now()))
	}

	private func write(_ edit: StoryLineEdit, to taskId: String) {
		guard let lines else {
			writeMessage = "No workspace is open"
			return
		}
		switch lines.apply(edit, to: taskId) {
		case .written:
			writeMessage = nil
		case let .refused(message):
			writeMessage = message
		}
		reload()
	}

	private func load(number: String) {
		guard let vault else {
			errorMessage = "No workspace is open"
			return
		}
		let today = DayText.string(now())
		do {
			let loaded = StoryMapping.content(try storyScreen(workspace: vault, number: number, today: today), today: today)
			let selected = accordion.selectedId
			content = loaded
			accordion = NextStepsAccordion(steps: loaded.nextSteps)
			accordion.select(id: selected)
			errorMessage = nil
		} catch {
			content = nil
			errorMessage = coreErrorMessage(error)
		}
		hours = state.flatMap { try? $0.storyHours(workspace: vault, number: number) }.map(StoryHours.init)
		togglWeek.load(source: togglSource, containing: now(), story: number)
	}
}
