import Foundation
import UserNotifications

/// The quiet notification for a fetch that brought new signals: no sound,
/// passive (it lands in Notification Center without lighting the screen),
/// and one per workspace, so a later fetch replaces the earlier one.
public enum QuietSignalNotification {
	/// The notification's text for `added` new signals, `waiting` new in all.
	public static func body(added: Int, waiting: Int) -> String {
		let came = added == 1 ? "1 new signal" : "\(added) new signals"
		return waiting > added ? "\(came) · \(waiting) to decide" : came
	}

	public static func post(workspaceId: String, workspaceName: String, added: Int, waiting: Int) {
		let content = UNMutableNotificationContent()
		content.title = workspaceName.isEmpty ? "Inbox" : "Inbox · \(workspaceName)"
		content.body = body(added: added, waiting: waiting)
		content.sound = nil
		content.interruptionLevel = .passive
		let request = UNNotificationRequest(identifier: "new-signals-\(workspaceId)", content: content, trigger: nil)
		Task {
			let center = UNUserNotificationCenter.current()
			let isAllowed = (try? await center.requestAuthorization(options: [.alert])) ?? false
			guard isAllowed else { return }
			try? await center.add(request)
		}
	}
}
