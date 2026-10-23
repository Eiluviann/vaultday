import Foundation
import UserNotifications

/// The long-timer check's question as a user notification: "Still working
/// on this?" with the task and the time counted. Asks for permission the
/// first time it has something to say.
@MainActor
public final class UserNotificationLongTimerAlert: LongTimerAlerting {
	/// The text a notification names the session by.
	private let describe: (String) -> String?

	public init(describe: @escaping (String) -> String?) {
		self.describe = describe
	}

	public func askStillWorking(sessionId: String, elapsedSeconds: Int) {
		let content = UNMutableNotificationContent()
		content.title = "Still working on this?"
		let subject = describe(sessionId) ?? "The timer"
		content.body = "\(subject) has run for \(DurationText.minutes(elapsedSeconds / 60))"
		content.sound = .default
		let request = UNNotificationRequest(identifier: "long-timer-\(sessionId)", content: content, trigger: nil)
		Task {
			let center = UNUserNotificationCenter.current()
			let isAllowed = (try? await center.requestAuthorization(options: [.alert, .sound])) ?? false
			guard isAllowed else { return }
			try? await center.add(request)
		}
	}
}
