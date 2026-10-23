import Foundation
import Observation
import VaultdayCore

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
	let dayPlan = DayPlanModel()
	let sessions = SessionModel()
	let focus: PaneFocusModel
	let dispatcher: KeyDispatcher
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
			dispatcher.setScheme(KeyScheme(settings.keyboardScheme))
			let store = WindowStateStore(layout: environment.layout)
			windowStateStore = store
			let windowState = store.load()
			focus.focus(windowState.focusedPane)
			if windowState.isSidebarCollapsed != focus.isSidebarCollapsed {
				focus.apply(.toggleSidebar)
			}
			workspace.restoreAtLaunch()
			reloadWorkspaceContent()
		} catch {
			setupError = error.localizedDescription
		}
	}

	/// The user picked a folder in the open panel.
	func grantFolder(_ url: URL) {
		workspace?.grantFolder(url)
		reloadWorkspaceContent()
	}

	/// Feeds one key press to the dispatcher; `true` when it was a binding or a pending prefix.
	func handle(_ stroke: KeyStroke) -> Bool {
		switch dispatcher.handle(stroke) {
		case let .action(action):
			focus.apply(action)
			saveWindowState()
			return true
		case .pending:
			return true
		case .unbound:
			return false
		}
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
		dispatcher.setScheme(KeyScheme(scheme))
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
	}

	func setCalendar(_ calendar: CalendarInfo, isPicked: Bool) {
		workspace?.setCalendar(id: calendar.id, title: calendar.title, isPicked: isPicked)
		calendarSource?.setPickedCalendars(workspace?.pickedCalendarIds ?? [])
	}

	func reloadWorkspaceContent() {
		guard let workspace else { return }
		dayPlan.load(from: workspace)
		sessions.load(from: workspace)
		focus.setItemCount(dayPlan.stories.reduce(0) { $0 + $1.nextSteps.count + $1.pendingInput.count }, in: .today)
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
