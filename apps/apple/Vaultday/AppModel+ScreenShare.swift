import SwiftUI
import VaultdayCore

/// Screen-share mode (PR 19) as the app wires it: the device setting, the
/// workspace a relaunch moves to and back from, the windows kept out of a
/// share, and the models that reduce what they hold. The rules themselves
/// are `ScreenShareModel`'s, tested without a window.
extension AppModel {
	/// Reads the device setting, enforces it on the workspace the app opened
	/// on, and wires the models and the windows to it. Called once at launch,
	/// after the timer surfaces exist.
	func configureScreenShare(settings: SettingsModel) {
		screenShare.persist = { [weak self] isOn, returnTo in
			self?.settings?.setScreenShare(isOn: isOn, returnTo: returnTo?.storedText)
		}
		screenShare.restore(isOn: settings.isScreenShareOn, returnTo: settings.screenShareReturnTo)
		windowSharing.isScreenShareOn = { [weak self] in self?.screenShare.isOn ?? false }
		longTimerAlert?.isScreenShareOn = { [weak self] in self?.screenShare.isOn ?? false }
		seedLastWorkWorkspace()
		leaveHiddenWorkspaceForScreenShare()
		applyScreenShare()
	}

	/// ⌘⇧S, or the header's toggle.
	func toggleScreenShare() {
		setScreenShare(!screenShare.isOn)
	}

	/// Turns the mode on or off. Turning it on is refused, with the reason in
	/// the header, unless a work workspace is shown; turning it off shows the
	/// selection a relaunch moved away from.
	func setScreenShare(_ isOn: Bool) {
		let change = screenShare.setOn(isOn, current: currentWorkspaceSelection, entries: workspaceEntries)
		apply(change)
		applyScreenShare()
	}

	/// Why the header's toggle is drawn disabled now, `nil` while it can be used.
	var screenShareStartDisabledReason: String? {
		screenShare.startDisabledReason(current: currentWorkspaceSelection, entries: workspaceEntries)
	}

	/// The mode is on and the open workspace is one it hides (a relaunch, a
	/// workspace added or removed in Settings): show the last work workspace,
	/// or turn the mode off when no work workspace is left.
	func leaveHiddenWorkspaceForScreenShare() {
		guard screenShare.isOn else { return }
		apply(screenShare.enforce(current: currentWorkspaceSelection, entries: workspaceEntries))
		if !screenShare.isOn { applyScreenShare() }
	}

	/// What the main window shows by workspace, `nil` before a workspace is open.
	var currentWorkspaceSelection: WorkspaceSelection? {
		guard let workspace, workspace.phase == .open else { return nil }
		return isShowingAll ? .all : .workspace(workspace.workspaceId)
	}

	/// Points every model and window that reduces itself at the mode.
	private func applyScreenShare() {
		let isOn = screenShare.isOn
		preview.isScreenShare = isOn
		peek.isScreenShare = isOn
		reports.isScreenShare = isOn
		story.reports.isScreenShare = isOn
		dayLog.hidesSessionLines = isOn
		// The close-out's reconciliation steps draw tracked minutes; it does not stay open.
		if isOn, closeOut.isPresented { closeOut.isPresented = false }
		syncSidePanelItemCount()
		if let workspace, workspace.phase == .open {
			quickAdd.attach(to: workspace, workspaces: screenShare.visible(workspaceEntries))
		}
		windowSharing.apply()
	}

	private func apply(_ change: ScreenShareChange) {
		switch change {
		case .none, .refused:
			break
		case let .select(.workspace(id)):
			if let entry = workspaceEntries.first(where: { $0.id == id }) { showWorkspace(entry) }
		case .select(.all):
			showAllWorkspaces()
		}
	}

	/// Where the mode goes from a hidden workspace: the one the app opened on, if it is work.
	private func seedLastWorkWorkspace() {
		let candidates = [workspace?.workspaceId, workspace?.lastSingleWorkspaceId].compactMap(\.self)
		if let entry = candidates.compactMap({ id in workspaceEntries.first { $0.id == id } }).first(where: { $0.sphere == .work }) {
			screenShare.noteShown(entry)
		}
	}
}
