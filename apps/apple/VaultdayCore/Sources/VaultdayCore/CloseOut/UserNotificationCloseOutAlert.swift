import Foundation
import UserNotifications

/// Posts the Monday close-out alert as a system notification as well, for
/// when the window is not in front. The core fires it once per week.
public enum UserNotificationCloseOutAlert {
	public static func post(_ alert: CloseOutAlert) {
		let content = UNMutableNotificationContent()
		content.title = "\(alert.workspaceName) · \(alert.title)"
		content.body = alert.missing.joined(separator: "\n")
		let request = UNNotificationRequest(identifier: "close-out-\(alert.workspaceName)-\(alert.weekStart)", content: content, trigger: nil)
		Task {
			let center = UNUserNotificationCenter.current()
			let isAllowed = (try? await center.requestAuthorization(options: [.alert])) ?? false
			guard isAllowed else { return }
			try? await center.add(request)
		}
	}
}
