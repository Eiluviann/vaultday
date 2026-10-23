import Foundation
import Observation

/// What fills the one main window: the setup flow or Home.
public enum MainWindowRoot: Equatable, Sendable {
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

	public private(set) var root: MainWindowRoot = .home
	/// Settable so a sheet's binding can clear it when the sheet closes.
	public var panel: MainWindowPanel?

	public init() {}

	/// Once the app document is open: a fresh user lands in setup. The root
	/// stays there while the steps add workspaces, until setup is left.
	public func launched(needsSetup: Bool) {
		root = needsSetup ? .setup : .home
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
	public func toggleSettings() {
		panel = panel == .settings ? nil : .settings
	}

	public func showSyncCheck() {
		panel = .syncCheck
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
