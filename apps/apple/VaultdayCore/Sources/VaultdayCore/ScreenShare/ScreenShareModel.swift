import Foundation
import Observation

/// What the main window shows by workspace: every workspace together, or one.
public enum WorkspaceSelection: Equatable, Hashable, Sendable {
	case all
	case workspace(String)

	private static let workspacePrefix = "workspace:"

	/// The text the core keeps, opaque to it: `all` or `workspace:<id>`.
	public var storedText: String {
		switch self {
		case .all: "all"
		case let .workspace(id): Self.workspacePrefix + id
		}
	}

	public init?(storedText: String) {
		if storedText == "all" {
			self = .all
		} else if storedText.hasPrefix(Self.workspacePrefix), storedText.count > Self.workspacePrefix.count {
			self = .workspace(String(storedText.dropFirst(Self.workspacePrefix.count)))
		} else {
			return nil
		}
	}
}

/// The places that show something screen-share mode hides. Every view that
/// draws one of them asks ``ScreenShareModel/hides(_:)``, so this list is the
/// one place to read what the mode covers, and the tests walk it.
///
/// The mini tracker and the menu-bar extra's window are not here: the mode
/// keeps them out of a capture (``WindowSharingRegistry``), so they keep
/// their elapsed time and durations as the design draws them. The close-out
/// ritual is not here either: it refuses to open while the mode is on.
public enum ScreenShareSurface: CaseIterable, Sendable {
	// Tracked time, private to the person.
	/// The Hours tab's daily target and its stories.
	case hoursTab
	/// The story screen's Hours block.
	case storyHours
	/// The Today column's Spent pill.
	case todayTotals
	/// The sidebar's tracked ring.
	case sidebarRing
	/// The Now card's elapsed clock, and a running subtask's.
	case nowCard
	/// The running clock in the menu bar itself: the system menu bar is in
	/// every full-screen capture, and no window setting keeps it out.
	case menuBarTitle
	/// The Toggl week's hours.
	case togglWeek
	/// The Day log's session lines.
	case dayLogSessions
	/// The long-timer notification's time counted.
	case longTimerAlert

	// Private content.
	/// An Inbox card: only the headline and the source icon stay.
	case signalCard
	/// The signal preview slide-over.
	case signalPreview
	/// The task preview slide-over.
	case taskPreview
	/// The Peek popover.
	case peek

	/// Whether the surface shows time the timer counted.
	public var isTrackedTime: Bool {
		switch self {
		case .hoursTab, .storyHours, .todayTotals, .sidebarRing, .nowCard, .menuBarTitle, .togglWeek, .dayLogSessions, .longTimerAlert:
			true
		case .signalCard, .signalPreview, .taskPreview, .peek:
			false
		}
	}
}

/// What turning the mode on or off asks the app to do about the workspace shown.
public enum ScreenShareChange: Equatable, Sendable {
	/// Nothing moves.
	case none
	/// Show this selection.
	case select(WorkspaceSelection)
	/// The mode did not change; the notice says why.
	case refused(String)
}

/// Screen-share mode for this Mac: what it hides, which workspaces stay
/// selectable, where the app goes when the mode starts and where it comes
/// back to when it ends. The mode is a per-device setting of the app-level
/// document; ``persist`` writes it, and nothing here reaches a workspace.
///
/// The mode starts only from a work workspace: the header's toggle is drawn
/// disabled on All workspaces and on a life workspace. While on: tracked time
/// in the main window shows the hand with "Private" instead of a value, the
/// life workspaces leave the switcher, Settings and the palette and refuse
/// selection, All workspaces is off, Inbox cards, previews and peeks keep
/// headlines and source icons only, reports offer their FTE-safe variant, and
/// the close-out ritual refuses to open.
@MainActor
@Observable
public final class ScreenShareModel {
	public static let allWorkspacesReason = "Screen-share mode needs a single workspace: pick one first"
	public static let hiddenWorkspaceNotice = "That workspace is hidden while screen-share mode is on"
	public static let startReason = "Screen-share mode starts from a work workspace"
	public static let closeOutNotice = "The close-out shows tracked time: turn off screen-share mode to open it"
	public static let noWorkWorkspaceNotice = "No work workspace is left to show: screen-share mode stays on until you turn it off"

	public private(set) var isOn = false
	/// Why the last switch or selection was refused; shown until dismissed.
	public private(set) var notice: String?
	/// The selection the mode moved the app away from, restored when it ends.
	public private(set) var returnTo: WorkspaceSelection?
	/// The work workspace shown last, where the mode goes from a hidden one.
	public private(set) var lastWorkWorkspaceId: String?

	/// Called after the mode or the selection to return to changed, to write them to the device's settings.
	@ObservationIgnored public var persist: (_ isOn: Bool, _ returnTo: WorkspaceSelection?) -> Void = { _, _ in }

	public init() {}

	/// Loads what the device settings kept; writes nothing.
	public func restore(isOn: Bool, returnTo storedText: String?) {
		self.isOn = isOn
		returnTo = isOn ? storedText.flatMap { WorkspaceSelection(storedText: $0) } : nil
		notice = nil
	}

	/// Why the mode cannot be turned on from `current` now, `nil` when it can
	/// (or when it is on, and can always be turned off). The header draws its
	/// toggle disabled with this as the tooltip.
	public func startDisabledReason(current: WorkspaceSelection?, entries: [WorkspaceEntry]) -> String? {
		guard !isOn else { return nil }
		guard let current, !isHidden(current, in: entries) else { return Self.startReason }
		return nil
	}

	/// Whether `sphere`'s workspaces may be listed now: Settings and setup's
	/// Workspaces and Accounts steps leave the life ones out while the mode is on.
	public func showsSphere(_ sphere: WorkspaceSphere) -> Bool {
		!isOn || sphere == .work
	}

	/// Refuses opening the close-out ritual with a notice while the mode is on:
	/// its reconciliation steps draw tracked minutes. `true` when refused.
	public func refusesClosingOut() -> Bool {
		guard isOn else { return false }
		notice = Self.closeOutNotice
		return true
	}

	/// Whether `surface` shows its private part now.
	public func hides(_ surface: ScreenShareSurface) -> Bool {
		isOn
	}

	/// Whether a mail fetch's quiet notification is posted: never while the
	/// mode is on. The Inbox and the sidebar badge count the new signals either way.
	public var postsSignalNotifications: Bool {
		!isOn
	}

	/// The header toggle's tooltip, with the toggle's key when it has one.
	public static func toggleTooltip(isOn: Bool, keyLabel: String?) -> String {
		if isOn {
			let offHint = keyLabel.map { "; \($0) turns it off" } ?? ""
			return "Screen-share mode is on: private workspaces are hidden and tracked time shows as private\(offHint)"
		}
		let keyHint = keyLabel.map { " (\($0))" } ?? ""
		return "Screen-share mode: hide other workspaces and silence their notifications\(keyHint)"
	}

	// MARK: Workspaces

	/// The workspaces the switcher, the palette and the quick-add picker list.
	public func visible(_ entries: [WorkspaceEntry]) -> [WorkspaceEntry] {
		isOn ? entries.filter { $0.sphere == .work } : entries
	}

	/// Whether `entry` may be shown now.
	public func canShow(_ entry: WorkspaceEntry) -> Bool {
		!isOn || entry.sphere == .work
	}

	/// The reason All workspaces is off, `nil` while it is on offer.
	public var allWorkspacesDisabledReason: String? {
		isOn ? Self.allWorkspacesReason : nil
	}

	/// Refuses showing `entry` with a notice when the mode hides it; `true` when refused.
	public func refusesShowing(_ entry: WorkspaceEntry) -> Bool {
		guard !canShow(entry) else { return false }
		notice = Self.hiddenWorkspaceNotice
		return true
	}

	/// Refuses showing every workspace together with its reason; `true` when refused.
	public func refusesShowingAll() -> Bool {
		guard let reason = allWorkspacesDisabledReason else { return false }
		notice = reason
		return true
	}

	/// A work workspace was shown: it is where the mode goes next time.
	public func noteShown(_ entry: WorkspaceEntry) {
		guard entry.sphere == .work else { return }
		lastWorkWorkspaceId = entry.id
	}

	public func dismissNotice() {
		notice = nil
	}

	// MARK: Switching

	/// Turns the mode on or off for the selection `current` shows (`nil`
	/// when no workspace is open). Turning it on is refused with
	/// ``startReason`` unless a work workspace is shown; nothing moves.
	/// Turning it off goes back to the selection a relaunch moved away from
	/// (``enforce(current:entries:)``) when it is still among `entries`.
	public func setOn(_ shouldBeOn: Bool, current: WorkspaceSelection?, entries: [WorkspaceEntry]) -> ScreenShareChange {
		notice = nil
		guard shouldBeOn != isOn else { return .none }
		if shouldBeOn {
			if let reason = startDisabledReason(current: current, entries: entries) {
				notice = reason
				return .refused(reason)
			}
			isOn = true
			persist(true, returnTo)
			return .none
		}
		isOn = false
		let back = returnTo.flatMap { selection in Self.exists(selection, in: entries) ? selection : nil }
		returnTo = nil
		persist(false, nil)
		return back.map(ScreenShareChange.select) ?? .none
	}

	/// At launch with the mode restored: a hidden selection (the app opened on
	/// a life workspace) moves to the last work workspace. With no work
	/// workspace left to move to, nothing moves and the mode stays on with
	/// ``noWorkWorkspaceNotice``: the person may be sharing their screen, so
	/// the mode never turns itself off and reveals tracked time.
	public func enforce(current: WorkspaceSelection?, entries: [WorkspaceEntry]) -> ScreenShareChange {
		guard isOn, let current, isHidden(current, in: entries) else { return .none }
		guard let target = workWorkspaceToShow(in: entries) else {
			notice = Self.noWorkWorkspaceNotice
			return .none
		}
		// An earlier move away from the same hidden selection keeps its way back.
		returnTo = returnTo ?? current
		persist(true, returnTo)
		return .select(.workspace(target.id))
	}

	// MARK: Helpers

	private func isHidden(_ selection: WorkspaceSelection, in entries: [WorkspaceEntry]) -> Bool {
		switch selection {
		case .all: true
		case let .workspace(id): entries.first { $0.id == id }?.sphere != .work
		}
	}

	private func workWorkspaceToShow(in entries: [WorkspaceEntry]) -> WorkspaceEntry? {
		let work = entries.filter { $0.sphere == .work }
		return work.first { $0.id == lastWorkWorkspaceId } ?? work.first
	}

	private static func exists(_ selection: WorkspaceSelection, in entries: [WorkspaceEntry]) -> Bool {
		switch selection {
		case .all: true
		case let .workspace(id): entries.contains { $0.id == id }
		}
	}
}
