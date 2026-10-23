import Foundation
import UserNotifications

/// The long-timer check's question as a user notification: "Still working
/// on this?" with the task and the time counted. Asks for permission the
/// first time it has something to say.
@MainActor
public final class UserNotificationLongTimerAlert: LongTimerAlerting {
	/// The text a notification names the session by.
	private let describe: (String) -> String?
	/// Screen-share mode: a banner on a shared screen names neither the task nor the time counted.
	public var isScreenShareOn: () -> Bool = { false }

	public init(describe: @escaping (String) -> String?) {
		self.describe = describe
	}

	/// The notification's text: the task and the time counted, or neither while sharing.
	static func body(subject: String?, elapsedSeconds: Int, isScreenShareOn: Bool) -> String {
		if isScreenShareOn { return "The timer is still running" }
		return "\(subject ?? "The timer") has run for \(DurationText.minutes(elapsedSeconds / 60))"
	}

	public func askStillWorking(sessionId: String, elapsedSeconds: Int) {
		let content = UNMutableNotificationContent()
		content.title = "Still working on this?"
		content.body = Self.body(subject: describe(sessionId), elapsedSeconds: elapsedSeconds, isScreenShareOn: isScreenShareOn())
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
