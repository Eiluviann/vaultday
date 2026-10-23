import Foundation
import Observation

/// What fills the one main window: the setup flow or Home, or nothing
/// until the launch decides which.
public enum MainWindowRoot: Equatable, Sendable {
	/// Before the app document is open: the window draws nothing, so a fresh
	/// user never sees an empty Home before setup.
	case undecided
	case setup
	case home
}

/// What sits over the main window's root as a sheet, inside the same window.
public enum MainWindowPanel: String, Identifiable, Sendable {
	/// ⌘,: the app-level choices and each workspace's Configure sheet.
	case settings
	/// Window › Sync Check: the two-Mac test's probes.
	case syncCheck

	public var id: String { rawValue }
}

/// The app has one window. Setup fills it in place of Home, and Settings and
/// the sync check open over either as a sheet; nothing opens a second window.
/// The menu-bar extra and the mini tracker are the only surfaces outside it.
@MainActor
@Observable
public final class MainWindowModel {
	/// The scene id of the main window; AppKit's window identifier starts with it.
	public static let sceneId = "home"

	public private(set) var root: MainWindowRoot = .undecided
	/// Settable so a sheet's binding can clear it when the sheet closes.
	public var panel: MainWindowPanel?
	/// The sheets a view inside the window has open (the close-out, a step's
	/// Configure sheet). While one is, the window cannot present a panel.
	public private(set) var openChildSheets: Set<UUID> = []

	public init() {}

	/// Whether a view inside the window has a sheet of its own open.
	public var hasChildSheet: Bool {
		!openChildSheets.isEmpty
	}

	/// Once the app document is open: a fresh user lands in setup. The root
	/// stays there while the steps add workspaces, until setup is left.
	public func launched(needsSetup: Bool) {
		root = needsSetup ? .setup : .home
	}

	/// The app document did not open: Home, whose Today column says why.
	public func launchFailed() {
		root = .home
	}

	/// The app menu's "Set up Vaultday…": the flow fills the window again.
	public func showSetup() {
		panel = nil
		root = .setup
	}

	/// The flow's last step, or its title bar's Close: Home fills the window.
	public func showHome() {
		root = .home
	}

	/// ⌘, opens Settings over whatever the window shows, and closes it again.
	/// While a child sheet is open the key is ignored and nothing changes:
	/// the window could not present Settings over that sheet.
	public func toggleSettings() {
		guard !hasChildSheet else { return }
		panel = panel == .settings ? nil : .settings
	}

	/// Window › Sync Check; ignored, as ⌘, is, while a child sheet is open.
	public func showSyncCheck() {
		guard !hasChildSheet else { return }
		panel = .syncCheck
	}

	/// A view inside the window presented a sheet of its own.
	public func childSheetOpened(_ id: UUID) {
		openChildSheets.insert(id)
	}

	/// That sheet closed.
	public func childSheetClosed(_ id: UUID) {
		openChildSheets.remove(id)
	}

	/// Esc or Done in a sheet.
	public func closePanel() {
		panel = nil
	}

	/// Whether the key window is the main window or a sheet or popover over
	/// it: `lineage` is the key window's identifier first, then its sheet
	/// parent's or parent window's, up to the topmost. A sheet over Home still
	/// counts as Home being frontmost, so the mini tracker stays hidden.
	public static func isMainWindow(lineage: [String?]) -> Bool {
		lineage.contains { $0?.hasPrefix(sceneId) == true }
	}
}
