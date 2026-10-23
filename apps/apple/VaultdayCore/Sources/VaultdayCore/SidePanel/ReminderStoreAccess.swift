import EventKit
import Foundation

/// One Reminders list.
public struct ReminderListInfo: Identifiable, Hashable, Sendable {
	/// The EventKit calendar identifier.
	public let id: String
	public let title: String
	/// The account the list lives in, such as "iCloud".
	public let sourceTitle: String

	public init(id: String, title: String, sourceTitle: String) {
		self.id = id
		self.title = title
		self.sourceTitle = sourceTitle
	}
}

/// One Reminders item, as the intake needs it.
public struct ReminderItem: Hashable, Sendable {
	/// This Mac's identifier, used to delete the item.
	public let id: String
	/// The identifier every device shares, written to the task line.
	public let externalId: String
	public let title: String
	/// The due day, `YYYY-MM-DD`.
	public let dueDate: String?
	public let hasDueTime: Bool
	public let isCompleted: Bool
	public let isRecurring: Bool
	public let hasLocationAlarm: Bool

	public init(id: String, externalId: String, title: String, dueDate: String? = nil, hasDueTime: Bool = false, isCompleted: Bool = false, isRecurring: Bool = false, hasLocationAlarm: Bool = false) {
		self.id = id
		self.externalId = externalId
		self.title = title
		self.dueDate = dueDate
		self.hasDueTime = hasDueTime
		self.isCompleted = isCompleted
		self.isRecurring = isRecurring
		self.hasLocationAlarm = hasLocationAlarm
	}
}

/// The Reminders operations the intake needs; EventKit in the app, a fake in tests.
public protocol ReminderStoreAccess: Sendable {
	var access: CalendarAccess { get }
	func requestFullAccess() async throws -> Bool
	func lists() -> [ReminderListInfo]
	/// The items of one list that are not completed.
	func incompleteReminders(inList listId: String) async -> [ReminderItem]
	/// Deletes an item; deleting one that is gone already is no error.
	func removeReminder(id: String) throws
}

/// ``ReminderStoreAccess`` over `EKEventStore`.
public final class EKReminderStoreAccess: ReminderStoreAccess, @unchecked Sendable {
	private let store = EKEventStore()

	public init() {}

	public var access: CalendarAccess {
		switch EKEventStore.authorizationStatus(for: .reminder) {
		case .fullAccess: .fullAccess
		case .writeOnly: .writeOnly
		case .notDetermined: .notDetermined
		default: .denied
		}
	}

	public func requestFullAccess() async throws -> Bool {
		try await store.requestFullAccessToReminders()
	}

	public func lists() -> [ReminderListInfo] {
		store.calendars(for: .reminder).map {
			ReminderListInfo(id: $0.calendarIdentifier, title: $0.title, sourceTitle: $0.source?.title ?? "")
		}
	}

	public func incompleteReminders(inList listId: String) async -> [ReminderItem] {
		guard let list = store.calendar(withIdentifier: listId) else { return [] }
		let predicate = store.predicateForIncompleteReminders(withDueDateStarting: nil, ending: nil, calendars: [list])
		return await withCheckedContinuation { continuation in
			store.fetchReminders(matching: predicate) { reminders in
				continuation.resume(returning: (reminders ?? []).map(Self.item))
			}
		}
	}

	public func removeReminder(id: String) throws {
		guard let reminder = store.calendarItem(withIdentifier: id) as? EKReminder else { return }
		try store.remove(reminder, commit: true)
	}

	private static func item(_ reminder: EKReminder) -> ReminderItem {
		let due = reminder.dueDateComponents
		let dueDate: String? = due.flatMap { parts in
			guard let year = parts.year, let month = parts.month, let day = parts.day else { return nil }
			return String(format: "%04d-%02d-%02d", year, month, day)
		}
		return ReminderItem(
			id: reminder.calendarItemIdentifier,
			externalId: reminder.calendarItemExternalIdentifier ?? reminder.calendarItemIdentifier,
			title: reminder.title ?? "",
			dueDate: dueDate,
			hasDueTime: due?.hour != nil,
			isCompleted: reminder.isCompleted,
			isRecurring: reminder.hasRecurrenceRules,
			hasLocationAlarm: reminder.alarms?.contains { $0.structuredLocation != nil || $0.proximity != .none } ?? false,
		)
	}
}
