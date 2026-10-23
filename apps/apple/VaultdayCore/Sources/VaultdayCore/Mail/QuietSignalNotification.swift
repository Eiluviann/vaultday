import Foundation
import UserNotifications

/// Asks the user whether Vaultday may post notifications. The Accounts
/// step asks once a mail account is saved, where the reason is plain;
/// a fetch never asks.
public protocol NotificationPermissionAsking: Sendable {
	/// Shows the system's request when it was never answered; `true` when allowed.
	func requestAlerts() async -> Bool
}

/// The system's notification permission.
public struct SystemNotificationPermission: NotificationPermissionAsking {
	public init() {}

	public func requestAlerts() async -> Bool {
		// The notification center exists only inside an app bundle (not in
		// a test runner or a command-line tool).
		guard Bundle.main.bundleURL.pathExtension == "app" else { return false }
		return (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert])) ?? false
	}
}

/// The quiet notification for a fetch that brought new signals: no sound,
/// passive (it lands in Notification Center without lighting the screen),
/// and one per workspace, so a later fetch replaces the earlier one. It is
/// posted only when the user already allowed notifications (the Accounts
/// step asks); a fetch never shows the permission request itself.
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
			let status = await center.notificationSettings().authorizationStatus
			guard status == .authorized || status == .provisional else { return }
			try? await center.add(request)
		}
	}
}
