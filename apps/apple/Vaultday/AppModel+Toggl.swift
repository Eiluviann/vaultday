import Foundation
import VaultdayCore

/// The Toggl and billing feature set's wiring: the queue's drain schedule
/// for the open workspace, and the per-workspace models Settings shows.
extension AppModel {
	/// Points the queue at the open workspace and drains it on its interval.
	/// While live access to Toggl is not allowed, a drain sends nothing.
	func restartTogglQueue() {
		guard let workspace, workspace.phase == .open else {
			togglQueue.stop()
			togglQueue.load(source: nil)
			return
		}
		togglQueue.load(source: workspace.togglQueueSource)
		togglQueue.start()
	}

	/// The Toggl panel of every workspace, for the Settings tab.
	func togglPanels() -> [TogglSettingsModel] {
		guard let coreSession, let workspaces else { return [] }
		return WorkspaceSphere.allCases
			.filter { screenShare.showsSphere($0) }
			.flatMap { workspaces.entries(in: $0) }
			.map { TogglSettingsModel(session: coreSession, workspaceId: $0.id, workspaceName: $0.name) }
	}

	/// The billing fields of a workspace's Configure sheet: only while the
	/// Toggl and billing feature set is on for it.
	func billingModel(for entry: WorkspaceEntry) -> BillingSettingsModel? {
		guard let coreSession else { return nil }
		let status = TogglSettingsModel(session: coreSession, workspaceId: entry.id, workspaceName: entry.name)
		guard status.isEnabled else { return nil }
		return BillingSettingsModel(root: entry.rootPath, workspaceId: entry.id, session: coreSession)
	}
}
