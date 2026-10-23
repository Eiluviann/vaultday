import Foundation
import VaultdayCore

/// The week close-out's wiring: the header's ritual button, the ritual
/// screen and the Monday alert, for the open workspace.
extension AppModel {
	/// The root whose `Leave.md` counts for the open workspace: its
	/// `leave_source`, else the life workspace.
	var leaveRoot: String? {
		guard let workspace, workspace.phase == .open, let workspaces else { return nil }
		let entries = WorkspaceSphere.allCases.flatMap { workspaces.entries(in: $0) }
		return LeaveSource.root(for: workspace.rootPath, among: entries)
	}

	/// The open workspace's close-out; `nil` until it is open.
	var closeOutSource: (any CloseOutSource)? {
		guard let workspace, workspace.phase == .open else { return nil }
		return workspace.closeOutSource(leaveRoot: leaveRoot)
	}

	/// What the header offers, and the Monday alert when the core says it is due.
	func refreshCloseOut() {
		let source = closeOutSource
		guard source != nil else {
			closeOutDue.clear()
			return
		}
		closeOutDue.refresh(source: source)
		let before = closeOutDue.alert
		closeOutDue.checkAlert(source: source)
		if let alert = closeOutDue.alert, alert != before {
			UserNotificationCloseOutAlert.post(alert)
		}
	}

	/// Opens the ritual screen on what the header offers.
	func openCloseOut() {
		guard let due = closeOutDue.due, let source = closeOutSource else { return }
		closeOutDue.dismissAlert()
		closeOut.open(source: source, kind: due.kind, date: due.date)
	}
}
