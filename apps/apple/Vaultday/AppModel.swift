import EventKit
import Foundation
import Observation
import OSLog
import SwiftUI
import VaultdayCore

/// The models behind the setup window's steps, built once the app document is open.
struct SetupModels {
	let onboarding: OnboardingModel
	let workspaces: WorkspacesModel
	let sync: SyncStepModel
	let accounts: AccountsStepModel
	let features: FeatureSetsStepModel
	let security: SecurityStepModel
	let rhythm: RhythmModel
	let aiLevel: AiLevelStepModel
	let aiProviders: AiProvidersStepModel
	let aiAssistance: AiAssistanceStepModel
	let ready: ReadyStepModel
}

/// What the main area of the Home window shows: the day (Today, Inbox and
/// the side panel) or the Browse table across all three.
enum MainScreen {
	case home
	case browse
}

/// The app's composition: the platform environment, the core session and
/// the models the views read. Built once at launch. Not a view; it never
/// touches the core's generated types either, only VaultdayCore's models.
@MainActor
@Observable
final class AppModel {
	/// Set when the platform services could not start (unsigned build,
	/// no App Group container, Keychain refused).
	private(set) var setupError: String?
	private(set) var workspace: WorkspaceModel?
	private(set) var settings: SettingsModel?
	/// The setup window's models; `nil` until the app document is open.
	private(set) var setup: SetupModels?
	let dayPlan = DayPlanModel()
	let sessions = SessionModel()
	let quickAdd = QuickAddModel()
	/// The side panel's Calendar tab and the Today column's three-day and week grids.
	let calendarPanel = CalendarPanelModel()
	let hours = HoursModel()
	let remindersIntake = RemindersIntakeModel(store: EKReminderStoreAccess())
	let browse = BrowseModel()
	let preview = PreviewModel()
	let peek = PeekModel()
	let dayLog = DayLogModel()
	/// The Backlog tab's list: the Backlog preset of Browse.
	let backlog = BrowseModel()
	/// Where peekable views sit, for the force click; not observed.
	@ObservationIgnored let peekTargets = PeekTargetRegistry()
	/// Browse takes the place of Today, Inbox and the side panel while shown (`gb`).
	private(set) var screen: MainScreen = .home
	/// The side panel's open tab.
	var sidePanelTab: SidePanelTab = .calendar
	/// Bumped to move keyboard focus into the Browse query line (`/`).
	private(set) var browseQueryFocusRequest = 0
	/// The All view's model; `nil` until the app document is open.
	private(set) var allWorkspaces: AllWorkspacesModel?
	let focus: PaneFocusModel
	let dispatcher: KeyDispatcher
	/// Strokes to effects: the register, undo and repeat, the change editor.
	let keyboard: KeyboardSession
	/// The command palette, rendered from the same key table.
	let palette: CommandPaletteModel
	/// The stories worked on last, for the palette's empty state.
	private var recentStories = RecentStories()
	/// Bumped to move keyboard focus into the Day log's append field (⌘L).
	private(set) var dayLogFocusRequest = 0
	/// Where the next quick-add task goes: after or before this task (`o`, `O`).
	private var insertAnchor: (taskId: String, isAfter: Bool)?
	/// Bumped to move keyboard focus into the quick-add field.
	private(set) var quickAddFocusRequest = 0
	/// The Home text fields that hold keyboard focus now, by name; while any
	/// does, bare keys go to the field.
	private var focusedTextFields: Set<String> = []

	/// A Home text field gained or lost keyboard focus.
	func setTextField(_ name: String, isFocused: Bool) {
		if isFocused { focusedTextFields.insert(name) } else { focusedTextFields.remove(name) }
	}
	/// The calendar line in the side panel: access, the private calendar, or why not.
	private(set) var calendarStatus = ""
	private(set) var pickableCalendars: [CalendarInfo] = []
	/// The last `vaultday://` link received; links do nothing yet.
	private(set) var lastLink: VaultdayLink?
	/// Set when an unsigned Debug build runs on an in-memory Keychain.
	private(set) var unsignedNotice: String?
	/// A problem with Keymap.yaml, shown instead of silently dropping keys.
	let keymapError: String?

	private var calendarSource: EventKitCalendarSource?
	private var storeChangeObserver: (any NSObjectProtocol)?
	private var environment: CoreEnvironment?
	private var windowStateStore: WindowStateStore?

	init() {
		let keymap: Keymap
		switch Self.loadKeymap() {
		case let .success(loaded):
			keymap = loaded
			keymapError = nil
		case let .failure(error):
			keymap = Keymap(bindings: [])
			keymapError = error.description
		}
		dispatcher = KeyDispatcher(keymap: keymap, scheme: .standard)
		focus = PaneFocusModel()
		let register = PlanRegister(plan: dayPlan) { text in
			NSPasteboard.general.clearContents()
			NSPasteboard.general.setString(text, forType: .string)
		}
		keyboard = KeyboardSession(dispatcher: dispatcher, focus: focus, register: register)
		palette = CommandPaletteModel(dispatcher: dispatcher)
		keyboard.selectedTaskId = { [weak self] in self?.selectedTaskId }
		keyboard.onAction = { [weak self] in self?.performAppAction($0) }
		keyboard.isEnabled = { [weak self] in self?.isEnabled($0) ?? false }
		keyboard.onOwnAction = { [weak self] in self?.registerChanged($0) }
	}

	/// Opens the environment and the app document, restores the folder grants.
	func launch() {
		guard workspace == nil, setupError == nil else { return }
		do {
			#if DEBUG
				let environment = try CoreEnvironment.forThisMac(allowsUnsignedFallback: true)
			#else
				let environment = try CoreEnvironment.forThisMac()
			#endif
			unsignedNotice = environment.unsignedFallbackNotice
			self.environment = environment
			let session = try CoreSession.open(environment)
			let workspace = WorkspaceModel(session: session)
			let settings = SettingsModel(session: session)
			self.workspace = workspace
			self.settings = settings
			let workspaces = WorkspacesModel(session: session)
			let rhythm = RhythmModel(session: session)
			let setup = SetupModels(
				onboarding: OnboardingModel(session: session, workspaces: workspaces),
				workspaces: workspaces,
				sync: SyncStepModel(session: session, workspaces: workspaces),
				accounts: AccountsStepModel(session: session),
				features: FeatureSetsStepModel(session: session),
				security: SecurityStepModel(session: session),
				rhythm: rhythm,
				aiLevel: AiLevelStepModel(session: session),
				aiProviders: AiProvidersStepModel(session: session),
				aiAssistance: AiAssistanceStepModel(session: session),
				ready: ReadyStepModel(session: session, rhythm: rhythm),
			)
			self.setup = setup
			allWorkspaces = AllWorkspacesModel(session: session)
			workspaces.onChange = { [weak self] in self?.workspacesChanged() }
			keyboard.setScheme(KeyScheme(settings.keyboardScheme))
			focus.hoverSelects = settings.hoverSelects
			let store = WindowStateStore(layout: environment.layout)
			windowStateStore = store
			let windowState = store.load()
			focus.focus(windowState.focusedPane)
			if windowState.isSidebarCollapsed != focus.isSidebarCollapsed {
				focus.apply(.toggleSidebar)
			}
			#if DEBUG
				addDebugWorkspace(workspaces)
			#endif
			workspace.restoreAtLaunch()
			startSidePanel()
			reloadWorkspaceContent()
			refreshSetupSteps()
		} catch {
			setupError = error.localizedDescription
		}
	}

	#if DEBUG
		/// Manual runs of an unsigned build start on an empty container each
		/// time: `VAULTDAY_DEBUG_WORKSPACE=<folder>` adds that folder as a work
		/// workspace without the open panel. Use a copy of the mock vault only.
		private func addDebugWorkspace(_ workspaces: WorkspacesModel) {
			guard let path = ProcessInfo.processInfo.environment["VAULTDAY_DEBUG_WORKSPACE"], !path.isEmpty,
			      workspaces.entries.isEmpty
			else { return }
			workspaces.addWorkspace(folder: URL(fileURLWithPath: path, isDirectory: true), sphere: .work)
		}
	#endif

	/// Whether the setup window opens at launch: the flow is not finished and no workspace exists yet.
	var needsSetup: Bool {
		guard let setup else { return false }
		return !setup.onboarding.isFinished && setup.workspaces.entries.isEmpty
	}

	/// The user picked a folder in the open panel: it becomes a work workspace, as the setup's Workspaces step makes one.
	func grantFolder(_ url: URL) {
		setup?.workspaces.addWorkspace(folder: url, sphere: .work)
	}

	/// Why the last folder could not be added, for the empty Today column.
	var grantMessage: String? { setup?.workspaces.message }

	/// The workspaces changed in the setup window: the open workspace follows.
	func workspacesChanged() {
		guard let workspace, let workspaces = setup?.workspaces else { return }
		let isOpenOneGone = workspace.phase == .open && !workspaces.entries.contains { $0.id == workspace.workspaceId }
		if isOpenOneGone {
			workspace.close()
		}
		if workspace.phase != .open {
			workspace.restoreAtLaunch()
		}
		reloadWorkspaceContent()
		refreshSetupSteps()
	}

	private func refreshSetupSteps() {
		guard let setup else { return }
		let entries = setup.workspaces.entries
		setup.accounts.refresh(workspaces: entries)
		setup.features.refresh(workspaces: entries)
		setup.security.refresh(workspaces: entries)
		setup.aiLevel.refresh(workspaces: entries)
		setup.aiAssistance.refresh(workspaces: entries)
		setup.ready.refresh(workspaces: entries)
		setup.sync.refreshGitWorkspaces()
	}

	/// The Ready step's hand-off: the setup is finished, and the main window
	/// opens on the last single workspace, else the first one.
	func finishSetup() {
		guard let setup, let workspace else { return }
		setup.onboarding.finish(workspaces: setup.workspaces.entries)
		workspace.close()
		workspace.restoreAtLaunch()
		reloadWorkspaceContent()
	}

	/// The day's settings, shared by the Rituals step and the Settings window.
	var rhythm: RhythmModel? { setup?.rhythm }

	/// The workspaces list, shared by the setup and the Settings window.
	var workspaces: WorkspacesModel? { setup?.workspaces }

	/// Feeds one key press to the dispatcher; `true` when it was a binding or a
	/// pending prefix. While any text field is being typed in (`isTyping`)
	/// only ⌘ and ⌃ strokes are bindings, so typing never runs a list command.
	func handle(_ stroke: KeyStroke, isTyping: Bool = false) -> Bool {
		if isTyping || !focusedTextFields.isEmpty, stroke.modifiers.isDisjoint(with: [.command, .control]) {
			return false
		}
		switch keyboard.handle(stroke) {
		case .action, .pending:
			saveWindowState()
			return true
		case .unbound:
			return false
		}
	}

	/// A key typed while the peek popover holds the keys: only the keymap's
	/// `open`, `peek` and `dismiss` act there, so the list underneath stays put.
	func handlePeekKey(_ stroke: KeyStroke) -> Bool {
		switch dispatcher.handle(stroke) {
		case let .action(action):
			switch action {
			case .open:
				if let taskId = peek.content?.taskId { openPreview(taskId: taskId) }
			case .peek, .dismiss:
				peek.dismiss()
			default:
				break
			}
			return true
		case .pending:
			return true
		case .unbound:
			return false
		}
	}

	/// Runs one action, from a key or from a control that names it.
	func perform(_ action: KeyAction) {
		keyboard.run(action)
		saveWindowState()
	}

	/// The match walk and the group jumps belong to the Browse table.
	private func isEnabled(_ action: KeyAction) -> Bool {
		![.nextMatch, .previousMatch, .nextGroup, .previousGroup, .toggleFold].contains(action) || screen == .browse
	}

	/// The task the selection is on: a card of the day or a row of Browse.
	var selectedTaskId: String? {
		selectedBrowseRowId ?? selectedItem?.taskId
	}

	/// The actions the keyboard session hands over: everything but the
	/// register, undo, repeat and the change keys.
	private func performAppAction(_ action: KeyAction) {
		switch action {
		case .showAllWorkspaces:
			showAllWorkspaces()
		case let .showWorkspace(place):
			showWorkspace(at: place - 1)
		case .openPalette:
			openPalette()
		case .addTask:
			insertAnchor = nil
			quickAddFocusRequest += 1
		case .addTaskAfter, .addTaskBefore:
			insertAnchor = selectedItem.map { ($0.taskId, action == .addTaskAfter) }
			quickAddFocusRequest += 1
		case .nextWorkspace:
			stepWorkspace(by: 1)
		case .previousWorkspace:
			stepWorkspace(by: -1)
		case .nextSidePanelTab:
			stepSidePanelTab(by: 1)
		case .previousSidePanelTab:
			stepSidePanelTab(by: -1)
		case .appendDayLog:
			showSidePanelTab(.dayLog)
			dayLogFocusRequest += 1
		case .replan:
			schedule(.replan)
		case .autoSchedule:
			schedule(.autoSchedule)
		case .togglePin:
			if let item = selectedItem { togglePin(taskId: item.taskId) }
		case .toggleTimer:
			toggleTimer()
		case .previousDay:
			showDay(offset: -1)
		case .nextDay:
			showDay(offset: 1)
		case .showToday:
			leaveBrowse()
			showToday()
		case .dismiss:
			if palette.isOpen {
				palette.close()
			} else if peek.subject != nil {
				peek.dismiss()
			} else if preview.isOpen {
				preview.close()
			} else if quickAdd.entry != nil {
				quickAdd.cancel()
			} else {
				dayPlan.dismissToast()
			}
		case let .focusPane(pane):
			if pane == .today { leaveBrowse() }
		case .open:
			openSelection()
		case .showBrowse:
			showBrowse()
		case .showDayLog:
			showSidePanelTab(.dayLog)
		case let .showSidePanelTab(place):
			if SidePanelTab.allCases.indices.contains(place - 1) { showSidePanelTab(SidePanelTab.allCases[place - 1]) }
		case .focusBrowseQuery:
			if screen == .browse { browseQueryFocusRequest += 1 }
		case .toggleFold:
			toggleFoldOfSelection()
		case .nextGroup:
			jumpGroup(by: 1)
		case .previousGroup:
			jumpGroup(by: -1)
		case .peek:
			if let subject = peekSubject { peek.toggle(subject) }
		case .openLink:
			openLinkOfSelection()
		case .openInClaudeCode:
			openSelectionInClaudeCode()
		default:
			// Movement is the focus model's; the register's keys never arrive here.
			break
		}
	}

	/// The keys that run `action` under the current scheme, for the caps beside controls.
	func keyLabel(_ action: KeyAction) -> String? {
		dispatcher.label(for: action)
	}

	// MARK: Browse, preview, peek and day log

	/// `gb`: the Browse table in place of the day.
	func showBrowse() {
		guard workspace?.phase == .open, !isShowingAll else { return }
		screen = .browse
		// One read of the vault serves the table, its counts and the Backlog tab.
		BrowseModel.refresh([browse, backlog])
		syncTodayItemCount()
	}

	/// Back to the day from Browse.
	func leaveBrowse() {
		guard screen == .browse else { return }
		screen = .home
		syncTodayItemCount()
	}

	/// The Today pane's item count follows what the main area shows.
	func syncTodayItemCount() {
		focus.setItemCount(screen == .browse ? browse.items.count : dayPlan.items.count, in: .today)
	}

	/// The side panel pane's item count follows its open tab.
	func syncSidePanelItemCount() {
		focus.setItemCount(sidePanelTab == .dayLog ? dayLog.visibleLines.count : 0, in: .sidePanel)
	}

	/// Opens a rail tab and puts keyboard focus in the side panel.
	func showSidePanelTab(_ tab: SidePanelTab) {
		sidePanelTab = tab
		if tab == .dayLog { dayLog.reload() }
		syncSidePanelItemCount()
		focus.focus(.sidePanel)
	}

	/// The selected line of the Browse table.
	var selectedBrowseItem: BrowseItem? {
		guard screen == .browse, let index = focus.selection(in: .today), browse.items.indices.contains(index) else { return nil }
		return browse.items[index]
	}

	/// The selected line of the Day log tab.
	var selectedLogLine: DayLogLine? {
		guard sidePanelTab == .dayLog, let index = focus.selection(in: .sidePanel), dayLog.visibleLines.indices.contains(index) else { return nil }
		return dayLog.visibleLines[index]
	}

	/// What `K` looks up: the selected task, story or log line of the focused pane.
	var peekSubject: PeekSubject? {
		switch focus.focusedPane {
		case .today:
			switch selectedBrowseItem {
			case let .row(row): return .task(row.id)
			case let .header(group): return group.storyNumber.map(PeekSubject.story)
			case nil: return selectedItem.map { .task($0.taskId) }
			}
		case .sidePanel:
			return selectedLogLine.map(PeekSubject.logLine)
		case .sidebar, .inbox:
			return nil
		}
	}

	/// ⏎ on the selection: a task opens its preview, a Browse group header folds.
	private func openSelection() {
		guard focus.focusedPane == .today else { return }
		switch selectedBrowseItem {
		case let .row(row):
			preview.open(taskId: row.id)
		case let .header(group):
			browse.toggleFold(group)
			syncTodayItemCount()
		case nil:
			if let item = selectedItem { openPreview(taskId: item.taskId) }
		}
	}

	/// Opens the preview of a task, from a peek's Open or a row's click.
	/// `gx`: the open preview's note, else the selected item's, in Obsidian.
	private func openLinkOfSelection() {
		guard let link = preview.content?.obsidianLink ?? peekSubject.flatMap({ peek.link(for: $0) }) else { return }
		NSWorkspace.shared.open(link)
	}

	/// `⌘⇧C`: the preview's Open in Claude Code, for the open preview's task or the selected one.
	private func openSelectionInClaudeCode() {
		guard let taskId = preview.content?.taskId ?? selectedTaskId else { return }
		handleLink(preview.claudeCodeLink(taskId: taskId))
	}

	func openPreview(taskId: String) {
		touchStory(ofTask: taskId)
		peek.dismiss()
		preview.open(taskId: taskId)
	}

	private func toggleFoldOfSelection() {
		guard let index = focus.selection(in: .today), let group = browse.group(at: index) else { return }
		browse.toggleFold(group)
		syncTodayItemCount()
		// A folded group's rows are gone; the selection stays on its header.
		if let header = browse.items.firstIndex(of: .header(group)) { focus.select(header, in: .today) }
	}

	private func jumpGroup(by step: Int) {
		guard let index = focus.selection(in: .today) else { return }
		focus.select(step > 0 ? browse.nextGroupStart(after: index) : browse.previousGroupStart(before: index), in: .today)
	}

	/// Appends the Day log field's text, about the selected task when there is one.
	func appendDayLog() {
		let task = selectedBrowseRowId ?? selectedItem?.taskId
		dayLog.contextTask = task
		dayLog.contextStory = selectedBrowseRowStory ?? selectedItem?.storyNumber
		if dayLog.append() {
			syncSidePanelItemCount()
		}
	}

	private var selectedBrowseRowId: String? {
		if case let .row(row) = selectedBrowseItem { return row.id }
		return nil
	}

	private var selectedBrowseRowStory: String? {
		if case let .row(row) = selectedBrowseItem { return row.storyNumber }
		return nil
	}

	/// Ticks a Browse row done and reads the table again.
	func completeBrowseTask(_ taskId: String) {
		dayPlan.tick(taskId: taskId)
		BrowseModel.refresh([browse, backlog])
		syncTodayItemCount()
		afterPlanChange()
	}

	/// The Open in Claude Code and Full story buttons: the links are registered
	/// but nothing listens yet, so they are recorded and logged.
	func handleLink(_ url: URL?) {
		guard let url else { return }
		Logger(subsystem: "com.eiluviann.vaultday", category: "link").info("link not handled yet: \(url.absoluteString, privacy: .public)")
		handle(url)
	}

	// MARK: Home

	/// Whether the Today column shows every workspace together.
	var isShowingAll: Bool { workspace?.scope == .all }

	/// The workspaces in the sidebar's order: work first, then by name.
	var workspaceEntries: [WorkspaceEntry] { setup?.workspaces.entries ?? [] }

	/// The open workspace's sidebar entry.
	var currentEntry: WorkspaceEntry? {
		workspaceEntries.first { $0.id == workspace?.workspaceId }
	}

	/// The open workspace's identity color.
	var accentColor: Color { currentEntry?.color ?? Tokens.Colors.wsBlue }

	/// The selected card of the Today list, when the list holds one.
	var selectedItem: PlanItem? {
		guard !isShowingAll, screen == .home, let index = focus.selection(in: .today) else { return nil }
		return dayPlan.items.indices.contains(index) ? dayPlan.items[index] : nil
	}

	func select(_ item: PlanItem) {
		guard let index = dayPlan.items.firstIndex(of: item) else { return }
		focus.select(index, in: .today)
	}

	/// The pointer entered a card: it selects it when Settings say hover selects.
	func hover(_ item: PlanItem) {
		guard let index = dayPlan.items.firstIndex(of: item) else { return }
		focus.hover(index, in: .today)
	}

	/// The checkbox: done through the register, so `u` puts it back.
	func completeTask(_ taskId: String) {
		keyboard.register.complete(taskId)
		afterPlanChange()
	}

	func moveTask(_ taskId: String, to destination: Int) {
		dayPlan.move(taskId: taskId, to: destination)
		afterPlanChange()
		if let index = dayPlan.items.firstIndex(where: { $0.taskId == taskId }) {
			focus.select(index, in: .today)
		}
	}

	/// The register changed the plan: the list, the panes and the selection follow.
	private func registerChanged(_ action: KeyAction) {
		let moved = action == .moveTaskUp || action == .moveTaskDown ? selectedTaskId : nil
		if screen == .browse { BrowseModel.refresh([browse, backlog]) }
		syncTodayItemCount()
		afterPlanChange()
		if let moved, let index = dayPlan.items.firstIndex(where: { $0.taskId == moved }) {
			focus.select(index, in: .today)
		}
	}

	func schedule(_ mode: ScheduleMode) {
		guard !isShowingAll else { return }
		dayPlan.schedule(mode)
		refreshSidePanel()
	}

	func showDay(offset: Int) {
		if isShowingAll {
			allWorkspaces?.showDay(offset: offset)
		} else {
			dayPlan.showDay(offset: offset)
			afterPlanChange()
		}
	}

	func showToday() {
		if isShowingAll {
			allWorkspaces?.showToday()
		} else {
			dayPlan.showToday()
			afterPlanChange()
		}
	}

	/// Space: starts the timer on the selected task, or pauses and resumes the running one.
	func toggleTimer() {
		if sessions.openSession != nil {
			sessions.togglePause()
		} else if let item = selectedItem, !item.isDone {
			startTimer(taskId: item.taskId)
		}
	}

	/// A task dropped on the Now card, or Space on a selected one.
	func startTimer(taskId: String) {
		// A drop carries any text; only a task of the list starts the timer.
		guard taskText(taskId) != nil else { return }
		touchStory(ofTask: taskId)
		let story = dayPlan.items.first { $0.taskId == taskId }?.storyNumber
		sessions.start(taskId: taskId, story: story)
		dayPlan.reload()
	}

	/// The task a session runs on, by its text, for the Now card.
	func taskText(_ taskId: String?) -> String? {
		guard let taskId else { return nil }
		for item in dayPlan.items {
			if item.taskId == taskId { return item.text }
			if let subtask = item.subtasks.first(where: { $0.taskId == taskId }) { return subtask.text }
		}
		return nil
	}

	/// Saves the quick-add confirmation and shows the new task.
	func saveQuickAdd() {
		let known = Set(dayPlan.items.map(\.taskId))
		guard quickAdd.save() else { return }
		dayPlan.reload()
		// `o` and `O`: the new task goes after or before the selection it was added from.
		if let anchor = insertAnchor,
		   let added = dayPlan.unfinishedItems.first(where: { !known.contains($0.taskId) }),
		   let index = dayPlan.unfinishedItems.firstIndex(where: { $0.taskId == anchor.taskId })
		{
			dayPlan.move(taskId: added.taskId, to: anchor.isAfter ? index + 1 : index)
		}
		insertAnchor = nil
		afterPlanChange()
	}

	/// ⌘0: every workspace together. The last single workspace stays the one the app opens on.
	func showAllWorkspaces() {
		guard let workspace else { return }
		workspace.showAll()
		reloadAllWorkspaces()
	}

	/// ⌘1 to ⌘9: the workspace at `index` in the sidebar's order.
	func showWorkspace(at index: Int) {
		guard workspaceEntries.indices.contains(index) else { return }
		showWorkspace(workspaceEntries[index])
	}

	func showWorkspace(_ entry: WorkspaceEntry) {
		guard let workspace else { return }
		keyboard.reset()
		recentStories.clear()
		workspace.switchTo(entry)
		// Closing the previous workspace ends its folder access; every granted folder opens again.
		setup?.workspaces.reload()
		reloadWorkspaceContent()
	}

	/// The rituals of the open workspace's sphere with when each is next due.
	func ritualRows(now: Date) -> [RitualRow] {
		guard let rhythm else { return [] }
		let sphere = workspaceEntries.first { $0.id == workspace?.workspaceId }?.sphere ?? .work
		return RitualSchedule.rows(
			RitualSchedule.rituals(for: sphere),
			time: { rhythm.time(of: $0) },
			day: { rhythm.day(of: $0) },
			now: now,
		)
	}

	/// Whether no mail account is connected; the Inbox column's empty state says so.
	var hasMailAccount: Bool {
		settings?.connections.contains { $0.kind == .mailAccount } ?? false
	}

	private func afterPlanChange() {
		syncTodayItemCount()
		if let workspace {
			sessions.load(from: workspace)
			workspace.refreshHealth()
		}
		refreshSidePanel()
	}

	// MARK: Side panel

	/// Whether EventKit grants full calendar access, so events are read and pins written.
	var hasCalendarAccess: Bool { calendarSource != nil }

	/// The padlock, `m` or `X`: pins the task's box on `date` (the shown
	/// day when `nil`) to the private calendar, or unpins it.
	func togglePin(taskId: String, on date: String? = nil) {
		calendarPanel.togglePin(taskId: taskId, on: date)
	}

	/// A task dropped on a day grid at `startMinute` of `date`: pinned there.
	func pin(taskId: String, startMinute: Int, on date: String) -> Bool {
		// A drop carries any text; only a task of the list or of a grid is pinned.
		let gridTasks = ([calendarPanel.day].compactMap(\.self) + calendarPanel.spanDays).flatMap(\.blocks).compactMap(\.taskId)
		guard taskText(taskId) != nil || gridTasks.contains(taskId) else { return false }
		return calendarPanel.pin(taskId: taskId, startMinute: startMinute, on: date)
	}

	/// The Today title's switch: the list, or the calendar over three days or a week.
	func showSpan(_ span: CalendarSpan) {
		calendarPanel.show(span)
	}

	func requestRemindersAccess() async {
		await remindersIntake.requestAccess()
		await runIntake()
	}

	func setIntakeList(_ list: ReminderListInfo, isPicked: Bool, story: String?) {
		workspace?.setIntakeList(id: list.id, title: list.title, isPicked: isPicked, story: story)
		Task { await runIntake() }
	}

	/// Once a minute: ended meetings become sessions, the now line moves, the intake runs.
	func minuteTick() async {
		calendarPanel.tick()
		if let workspace, workspace.phase == .open {
			hours.load(from: workspace, date: dayPlan.date)
		}
		await runIntake()
	}

	/// Connects the calendars when access was granted before, and follows
	/// changes to the calendar and Reminders stores.
	private func startSidePanel() {
		calendarPanel.onPlanChanged = { [weak self] in self?.calendarPlanChanged() }
		calendarPanel.keyLabel = { [weak self] in self?.keyLabel($0) }
		if EKEventStore.authorizationStatus(for: .event) == .fullAccess {
			connectCalendar(EKEventStoreAccess())
		}
		remindersIntake.refreshLists()
		storeChangeObserver = NotificationCenter.default.addObserver(forName: .EKEventStoreChanged, object: nil, queue: .main) { [weak self] _ in
			Task { @MainActor in
				self?.remindersIntake.refreshLists()
				self?.calendarPanel.reload()
				await self?.runIntake()
			}
		}
	}

	/// A pin changed the plan: the list shows it, and pinned work that
	/// covered unlocked boxes has the core re-plan them around it.
	private func calendarPlanChanged() {
		dayPlan.reload()
		if calendarPanel.needsReplan {
			calendarPanel.needsReplan = false
			schedule(.replan)
		}
		afterPlanChange()
	}

	private func refreshSidePanel() {
		guard let workspace, workspace.phase == .open, !dayPlan.date.isEmpty else {
			calendarPanel.clear()
			hours.clear()
			return
		}
		calendarPanel.load(from: workspace, date: dayPlan.date)
		hours.load(from: workspace, date: dayPlan.date)
	}

	private func runIntake() async {
		guard let workspace, workspace.phase == .open, !isShowingAll else { return }
		if let run = await remindersIntake.run(workspace: workspace), run.filed > 0 {
			dayPlan.reload()
			afterPlanChange()
		}
	}

	/// What Home reads from the Settings window; a change reloads the day.
	var homeSettingsSignature: String {
		guard let rhythm else { return "" }
		let hours = rhythm.workingHours
		return [rhythm.newTaskPosition.rawValue, rhythm.overrunWarning.rawValue, hours.start.text, hours.end.text, "\(rhythm.workloadThresholdMinutes)"]
			.joined(separator: "|")
	}

	private func reloadAllWorkspaces() {
		guard let allWorkspaces else { return }
		allWorkspaces.newTaskPosition = rhythm?.newTaskPosition ?? .bottom
		allWorkspaces.load(entries: workspaceEntries, openWorkspace: workspace)
	}

	private func busySpans(on day: String) -> [BusySpan] {
		calendarSource?.busySpans(on: day) ?? []
	}

	/// Removes the open workspace from Vaultday on every device; the folder stays.
	func removeWorkspace() {
		workspace?.removeWorkspace()
		reloadWorkspaceContent()
	}

	func toggleSidebar() {
		focus.apply(.toggleSidebar)
		saveWindowState()
	}

	func setKeyboardScheme(_ scheme: KeyboardSchemeSetting) {
		settings?.setKeyboardScheme(scheme)
		keyboard.setScheme(KeyScheme(scheme))
	}

	func setHoverSelects(_ isOn: Bool) {
		settings?.setHoverSelects(isOn)
		focus.hoverSelects = isOn
	}

	// MARK: Palette

	/// ⌘K: the palette, with the stories worked on last and the workspaces.
	func openPalette() {
		let dayStories = dayPlan.items.compactMap { item in
			item.storyNumber.map { PaletteStory(number: $0, title: item.storyTitle ?? $0) }
		}
		palette.recentStories = recentStories.filled(with: dayStories)
		palette.workspaces = workspaceEntries.map { PaletteWorkspace(id: $0.id, name: $0.name) }
		palette.open()
	}

	/// Runs the palette's choice: a command, a story in Browse, a workspace.
	func choosePaletteRow(_ row: PaletteRow? = nil) {
		switch palette.choose(row) {
		case let .action(action):
			perform(action)
		case let .story(number):
			showBrowse()
			browse.queryText = "story:\(number)"
			browse.submitQuery()
			syncTodayItemCount()
		case let .workspace(id):
			if let entry = workspaceEntries.first(where: { $0.id == id }) { showWorkspace(entry) }
		case nil:
			break
		}
	}

	/// The story of a task the user opened, peeked or timed moves to the front.
	private func touchStory(ofTask taskId: String) {
		guard let item = dayPlan.items.first(where: { $0.taskId == taskId }), let number = item.storyNumber else { return }
		recentStories.touch(PaletteStory(number: number, title: item.storyTitle ?? number))
	}

	private func stepWorkspace(by step: Int) {
		let entries = workspaceEntries
		guard !entries.isEmpty else { return }
		let current = entries.firstIndex { $0.id == workspace?.workspaceId } ?? (step > 0 ? -1 : 0)
		let target = (current + step + entries.count) % entries.count
		showWorkspace(entries[target])
	}

	private func stepSidePanelTab(by step: Int) {
		let tabs = SidePanelTab.allCases
		guard let current = tabs.firstIndex(of: sidePanelTab) else { return }
		showSidePanelTab(tabs[(current + step + tabs.count) % tabs.count])
	}

	func handle(_ url: URL) {
		lastLink = VaultdayLink(url)
	}

	/// Asks for full calendar access, then makes sure the private calendar exists in iCloud.
	func requestCalendarAccess() async {
		let store = EKEventStoreAccess()
		do {
			guard try await store.requestFullAccess() else {
				calendarStatus = CalendarRefusal.accessNotGranted.localizedDescription
				return
			}
		} catch {
			calendarStatus = error.localizedDescription
			return
		}
		connectCalendar(store)
		reloadWorkspaceContent()
	}

	private func connectCalendar(_ store: EKEventStoreAccess) {
		let record: any PrivateCalendarRecord = environment.map { FilePrivateCalendarRecord(layout: $0.layout) } ?? InMemoryPrivateCalendarRecord()
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: workspace?.pickedCalendarIds ?? [], record: record)
		calendarSource = source
		pickableCalendars = source.pickableCalendars()
		do {
			_ = try source.ensurePrivateCalendar()
			calendarStatus = "Time boxes go to the private Vaultday calendar in iCloud"
		} catch {
			calendarStatus = error.localizedDescription
		}
		calendarPanel.attach(calendar: source)
	}

	func setCalendar(_ calendar: CalendarInfo, isPicked: Bool) {
		workspace?.setCalendar(id: calendar.id, title: calendar.title, isPicked: isPicked)
		calendarSource?.setPickedCalendars(workspace?.pickedCalendarIds ?? [])
		refreshSidePanel()
	}

	func reloadWorkspaceContent() {
		guard let workspace else { return }
		if let rhythm {
			dayPlan.newTaskPosition = rhythm.newTaskPosition
			dayPlan.overrunWarning = rhythm.overrunWarning
			dayPlan.workingHours = rhythm.workingHours
		}
		dayPlan.busyProvider = { [weak self] day in self?.busySpans(on: day) ?? [] }
		calendarSource?.setPickedCalendars(workspace.pickedCalendarIds)
		dayPlan.load(from: workspace)
		sessions.load(from: workspace)
		quickAdd.attach(to: workspace, workspaces: workspaceEntries)
		workspace.refreshHealth()
		let presetFile = environment.map { $0.layout.browsePresetsFile(workspaceId: workspace.workspaceId) }
		browse.load(from: workspace, presetFile: presetFile)
		if let preset = browse.presets.first(where: { $0.name == "Backlog" }) {
			backlog.load(sharingReadOf: browse, query: preset.query)
		}
		preview.attach(to: workspace)
		peek.attach(to: workspace)
		dayLog.load(from: workspace)
		syncTodayItemCount()
		syncSidePanelItemCount()
		refreshSidePanel()
		Task { await runIntake() }
		if isShowingAll {
			reloadAllWorkspaces()
		}
	}

	private func saveWindowState() {
		windowStateStore?.save(WindowState(isSidebarCollapsed: focus.isSidebarCollapsed, focusedPane: focus.focusedPane))
	}

	private static func loadKeymap() -> Result<Keymap, KeymapError> {
		guard let url = Bundle.main.url(forResource: "Keymap", withExtension: "yaml"),
		      let text = try? String(contentsOf: url, encoding: .utf8)
		else { return .failure(KeymapError(line: 0, reason: "Keymap.yaml is missing from the app bundle")) }
		return Keymap.parse(text)
	}
}
