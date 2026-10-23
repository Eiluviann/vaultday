import EventKit
import Foundation

/// What kind of account a calendar belongs to.
public enum CalendarSourceKind: Sendable, Hashable {
	/// iCloud Calendar (CalDAV, titled "iCloud").
	case iCloud
	/// An Exchange account; never a write target.
	case exchange
	/// Another CalDAV account.
	case otherCalDAV
	/// Calendars on this Mac only.
	case local
	/// Subscribed or birthday calendars.
	case readOnly
}

/// One account of the calendar store.
public struct CalendarSourceInfo: Sendable, Hashable {
	public let id: String
	public let title: String
	public let kind: CalendarSourceKind

	public init(id: String, title: String, kind: CalendarSourceKind) {
		self.id = id
		self.title = title
		self.kind = kind
	}
}

/// One calendar of the store.
public struct CalendarInfo: Sendable, Hashable, Identifiable {
	public let id: String
	public let title: String
	public let source: CalendarSourceInfo
	public let allowsModifications: Bool

	public init(id: String, title: String, source: CalendarSourceInfo, allowsModifications: Bool) {
		self.id = id
		self.title = title
		self.source = source
		self.allowsModifications = allowsModifications
	}
}

/// One event as the store holds it.
public struct StoredEvent: Sendable, Hashable {
	public let id: String
	public let calendarId: String
	public let title: String
	public let start: Date
	public let end: Date
	public let isAllDay: Bool
	public let url: URL?
	/// The occurrence's identity that a rename or a move keeps; `nil` when
	/// the store has no external identifier.
	public let meetingKey: String?
	/// `false` for an invitation the user declined, a cancelled event, or
	/// time that shows as free: no meeting the user sat in.
	public let isAttended: Bool

	public init(id: String, calendarId: String, title: String, start: Date, end: Date, isAllDay: Bool, url: URL?, meetingKey: String? = nil, isAttended: Bool = true) {
		self.id = id
		self.calendarId = calendarId
		self.title = title
		self.start = start
		self.end = end
		self.isAllDay = isAllDay
		self.url = url
		self.meetingKey = meetingKey
		self.isAttended = isAttended
	}
}

/// The app's calendar permission.
public enum CalendarAccess: Sendable, Equatable {
	case notDetermined
	case fullAccess
	case writeOnly
	case denied
}

/// The calendar store operations the calendar source needs; EventKit in the
/// app, a fake in tests.
public protocol EventStoreAccess: Sendable {
	var access: CalendarAccess { get }
	func requestFullAccess() async throws -> Bool
	func sources() -> [CalendarSourceInfo]
	func calendars() -> [CalendarInfo]
	func createCalendar(title: String, in source: CalendarSourceInfo) throws -> CalendarInfo
	func events(from start: Date, to end: Date, calendarIds: [String]) -> [StoredEvent]
	/// Saves a new event (`id == nil`) or changes an existing one; returns its id.
	func saveEvent(id: String?, calendarId: String, title: String, start: Date, end: Date, url: URL) throws -> String
	func removeEvent(id: String) throws
}

/// ``EventStoreAccess`` over `EKEventStore`.
public final class EKEventStoreAccess: EventStoreAccess, @unchecked Sendable {
	private let store = EKEventStore()

	public init() {}

	public var access: CalendarAccess {
		switch EKEventStore.authorizationStatus(for: .event) {
		case .fullAccess: .fullAccess
		case .writeOnly: .writeOnly
		case .notDetermined: .notDetermined
		default: .denied
		}
	}

	public func requestFullAccess() async throws -> Bool {
		try await store.requestFullAccessToEvents()
	}

	public func sources() -> [CalendarSourceInfo] {
		store.sources.map(Self.sourceInfo)
	}

	public func calendars() -> [CalendarInfo] {
		store.calendars(for: .event).map(Self.calendarInfo)
	}

	public func createCalendar(title: String, in source: CalendarSourceInfo) throws -> CalendarInfo {
		guard let ekSource = store.sources.first(where: { $0.sourceIdentifier == source.id }) else {
			throw CalendarRefusal.iCloudCalendarOff
		}
		let calendar = EKCalendar(for: .event, eventStore: store)
		calendar.title = title
		calendar.source = ekSource
		try store.saveCalendar(calendar, commit: true)
		return Self.calendarInfo(calendar)
	}

	public func events(from start: Date, to end: Date, calendarIds: [String]) -> [StoredEvent] {
		let calendars = store.calendars(for: .event).filter { calendarIds.contains($0.calendarIdentifier) }
		guard !calendars.isEmpty else { return [] }
		let predicate = store.predicateForEvents(withStart: start, end: end, calendars: calendars)
		return store.events(matching: predicate).map { event in
			StoredEvent(
				id: event.calendarItemIdentifier,
				calendarId: event.calendar.calendarIdentifier,
				title: event.title ?? "",
				start: event.startDate,
				end: event.endDate,
				isAllDay: event.isAllDay,
				url: event.url,
				meetingKey: Self.meetingKey(of: event),
				isAttended: Self.isAttended(event),
			)
		}
	}

	/// The external identifier every device and server shares; for a repeating
	/// event, with the occurrence's original start, which stays put when that
	/// one occurrence is moved. A single event's start is left out, so moving
	/// it keeps its key.
	private static func meetingKey(of event: EKEvent) -> String? {
		guard let external = event.calendarItemExternalIdentifier, !external.isEmpty else { return nil }
		guard event.hasRecurrenceRules || event.isDetached, let occurrence = event.occurrenceDate else { return external }
		return "\(external)@\(Int(occurrence.timeIntervalSince1970))"
	}

	private static func isAttended(_ event: EKEvent) -> Bool {
		if event.availability == .free || event.status == .canceled { return false }
		let me = event.attendees?.first { $0.isCurrentUser }
		return me?.participantStatus != .declined
	}

	public func saveEvent(id: String?, calendarId: String, title: String, start: Date, end: Date, url: URL) throws -> String {
		guard let calendar = store.calendar(withIdentifier: calendarId) else {
			throw CalendarFailure.UnknownCalendar(name: calendarId)
		}
		let event = id.flatMap { store.calendarItem(withIdentifier: $0) as? EKEvent } ?? EKEvent(eventStore: store)
		event.calendar = calendar
		event.title = title
		event.startDate = start
		event.endDate = end
		event.url = url
		try store.save(event, span: .thisEvent, commit: true)
		return event.calendarItemIdentifier
	}

	public func removeEvent(id: String) throws {
		guard let event = store.calendarItem(withIdentifier: id) as? EKEvent else { return }
		try store.remove(event, span: .thisEvent, commit: true)
	}

	private static func sourceInfo(_ source: EKSource) -> CalendarSourceInfo {
		let kind: CalendarSourceKind = switch source.sourceType {
		case .calDAV where source.title == "iCloud": .iCloud
		case .calDAV: .otherCalDAV
		case .exchange: .exchange
		case .local: .local
		default: .readOnly
		}
		return CalendarSourceInfo(id: source.sourceIdentifier, title: source.title, kind: kind)
	}

	private static func calendarInfo(_ calendar: EKCalendar) -> CalendarInfo {
		let source = calendar.source.map(sourceInfo) ?? CalendarSourceInfo(id: "", title: "", kind: .readOnly)
		return CalendarInfo(
			id: calendar.calendarIdentifier,
			title: calendar.title,
			source: source,
			allowsModifications: calendar.allowsContentModifications,
		)
	}
}
