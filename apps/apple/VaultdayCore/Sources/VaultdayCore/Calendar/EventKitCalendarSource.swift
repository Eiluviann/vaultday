import Foundation

/// Why the private calendar cannot be used; each case reads as a message for the user.
public enum CalendarRefusal: Error, Equatable, Sendable, LocalizedError {
	/// Calendar access was not granted in full.
	case accessNotGranted
	/// No iCloud calendar account on this Mac.
	case iCloudCalendarOff

	public var errorDescription: String? {
		switch self {
		case .accessNotGranted:
			"Vaultday needs full access to your calendars to read your events and keep time boxes. Allow it in System Settings › Privacy & Security › Calendars."
		case .iCloudCalendarOff:
			"Vaultday keeps time boxes in a private \"Vaultday\" calendar in iCloud, and iCloud Calendar is off on this Mac. Turn on Calendars in System Settings › Apple Account › iCloud, then try again."
		}
	}
}

/// Where this Mac remembers which calendar is the private one.
public protocol PrivateCalendarRecord: Sendable {
	func load() -> String?
	func save(_ calendarId: String)
}

/// ``PrivateCalendarRecord`` in a file of the App Group container's volatile
/// folder; losing it only means the calendar is looked up by name again.
public struct FilePrivateCalendarRecord: PrivateCalendarRecord {
	public let file: URL

	public init(layout: AppGroupLayout) {
		file = layout.volatileFolder.appending(path: "private-calendar-id")
	}

	public func load() -> String? {
		(try? String(contentsOf: file, encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines)
	}

	public func save(_ calendarId: String) {
		try? Data(calendarId.utf8).write(to: file, options: [.atomic])
	}
}

/// ``PrivateCalendarRecord`` in memory, for tests.
public final class InMemoryPrivateCalendarRecord: PrivateCalendarRecord, @unchecked Sendable {
	private let lock = NSLock()
	private var calendarId: String?

	public init(_ calendarId: String? = nil) {
		self.calendarId = calendarId
	}

	public func load() -> String? { lock.withLock { calendarId } }
	public func save(_ calendarId: String) { lock.withLock { self.calendarId = calendarId } }
}

/// The core's `CalendarSource` over EventKit.
///
/// - Reads cover only the calendars the workspace picked, without all-day
///   events and events without length.
/// - Time boxes go only to the private "Vaultday" calendar, and that calendar
///   is created only in the iCloud CalDAV account; with iCloud Calendar off
///   the write is refused with ``CalendarRefusal/iCloudCalendarOff``.
///   A "Vaultday" calendar in any other account, Exchange above all, is
///   never written to, nor is one that does not allow modifications. The
///   calendar this Mac created or first adopted (one another Mac created,
///   arriving through iCloud) is remembered, so a calendar of that name that
///   appears later, shared by someone else, never takes its place.
/// - Each time box's event carries `vaultday://time-box/<id>` as its URL,
///   which is how it is found again.
public final class EventKitCalendarSource: CalendarPort, @unchecked Sendable {
	public static let privateCalendarTitle = "Vaultday"

	private let store: any EventStoreAccess
	private let record: any PrivateCalendarRecord
	private let timeZone: TimeZone
	private let lock = NSLock()
	private var pickedCalendarIds: Set<String>

	public init(
		store: any EventStoreAccess,
		pickedCalendarIds: Set<String>,
		record: any PrivateCalendarRecord = InMemoryPrivateCalendarRecord(),
		timeZone: TimeZone = .current,
	) {
		self.store = store
		self.record = record
		self.pickedCalendarIds = pickedCalendarIds
		self.timeZone = timeZone
	}

	/// Replaces the calendars reads cover.
	public func setPickedCalendars(_ ids: Set<String>) {
		lock.withLock { pickedCalendarIds = ids }
	}

	/// The calendars the user can pick from: every calendar but the private one.
	public func pickableCalendars() -> [CalendarInfo] {
		let privateId = privateCalendar()?.id
		return store.calendars().filter { $0.id != privateId }
	}

	/// The private calendar, created in iCloud when it does not exist yet.
	public func ensurePrivateCalendar() throws(CalendarRefusal) -> CalendarInfo {
		guard store.access == .fullAccess else { throw .accessNotGranted }
		if let existing = privateCalendar() {
			record.save(existing.id)
			return existing
		}
		guard let iCloud = store.sources().first(where: { $0.kind == .iCloud }) else {
			throw .iCloudCalendarOff
		}
		do {
			let created = try store.createCalendar(title: Self.privateCalendarTitle, in: iCloud)
			record.save(created.id)
			return created
		} catch {
			throw .iCloudCalendarOff
		}
	}

	// MARK: CalendarPort

	public func events(from: String, to: String) throws -> [CalendarEvent] {
		guard store.access == .fullAccess else {
			throw CalendarFailure.Unavailable(reason: CalendarRefusal.accessNotGranted.localizedDescription)
		}
		guard let start = startOfDay(from), let lastDay = startOfDay(to),
		      let end = calendar.date(byAdding: .day, value: 1, to: lastDay), start < end
		else {
			throw CalendarFailure.Unavailable(reason: "\(from) to \(to) is not a range of days")
		}
		let picked = lock.withLock { pickedCalendarIds }
		let calendars = store.calendars().filter { picked.contains($0.id) }
		let titles = Dictionary(calendars.map { ($0.id, $0.title) }, uniquingKeysWith: { first, _ in first })
		return store.events(from: start, to: end, calendarIds: calendars.map(\.id))
			.filter { !$0.isAllDay && $0.end > $0.start }
			.map { event in
				CalendarEvent(
					id: event.id,
					calendar: titles[event.calendarId] ?? "",
					title: event.title,
					start: civilText(event.start),
					end: civilText(event.end),
				)
			}
	}

	/// The picked calendars' timed events on `day` (`YYYY-MM-DD`) as busy
	/// minutes of that day, by start; an event that crosses midnight is cut
	/// at the day's edges. Empty without calendar access.
	public func busySpans(on day: String) -> [BusySpan] {
		guard let events = try? events(from: day, to: day) else { return [] }
		return events.compactMap { event in
			let start = event.start.hasPrefix(day) ? minuteOfDay(event.start) : 0
			let end = event.end.hasPrefix(day) ? minuteOfDay(event.end) : 24 * 60
			guard let start, let end, end > start else { return nil }
			return BusySpan(startMinute: start, endMinute: end)
		}
		.sorted { ($0.startMinute, $0.endMinute) < ($1.startMinute, $1.endMinute) }
	}

	public func writeTimeBox(calendar name: String, title: String, timeBox: CalendarTimeBox) throws {
		guard name == Self.privateCalendarTitle else { throw CalendarFailure.UnknownCalendar(name: name) }
		let target: CalendarInfo
		do {
			target = try ensurePrivateCalendar()
		} catch {
			throw CalendarFailure.Unavailable(reason: error.localizedDescription)
		}
		guard let start = civilDate(timeBox.start) else {
			throw CalendarFailure.Unavailable(reason: "\(timeBox.start) is not a date and time")
		}
		let end = start.addingTimeInterval(TimeInterval(timeBox.durationMinutes) * 60)
		let existing = timeBoxEvent(timeBox.id, in: target)
		do {
			_ = try store.saveEvent(id: existing?.id, calendarId: target.id, title: title, start: start, end: end, url: Self.timeBoxURL(timeBox.id))
		} catch {
			throw CalendarFailure.Unavailable(reason: (error as NSError).localizedDescription)
		}
	}

	public func deleteTimeBox(calendar name: String, id: String) throws {
		guard store.access == .fullAccess else {
			throw CalendarFailure.Unavailable(reason: CalendarRefusal.accessNotGranted.localizedDescription)
		}
		guard name == Self.privateCalendarTitle, let target = privateCalendar() else {
			throw CalendarFailure.UnknownCalendar(name: name)
		}
		guard let event = timeBoxEvent(id, in: target) else {
			throw CalendarFailure.TimeBoxNotFound(calendar: name, id: id)
		}
		do {
			try store.removeEvent(id: event.id)
		} catch {
			throw CalendarFailure.Unavailable(reason: (error as NSError).localizedDescription)
		}
	}

	// MARK: Helpers

	static func timeBoxURL(_ id: String) -> URL {
		var components = URLComponents()
		components.scheme = "vaultday"
		components.host = "time-box"
		components.path = "/\(id)"
		return components.url ?? URL(fileURLWithPath: "/")
	}

	/// The private calendar: the one recorded on this Mac while it is still a
	/// writable iCloud calendar; without a record, a writable iCloud calendar
	/// of that name (another Mac created it).
	private func privateCalendar() -> CalendarInfo? {
		let calendars = store.calendars()
		if let recorded = record.load(), let calendar = calendars.first(where: { $0.id == recorded }) {
			return isUsable(calendar) ? calendar : nil
		}
		return calendars.first { $0.title == Self.privateCalendarTitle && isUsable($0) }
	}

	private func isUsable(_ calendar: CalendarInfo) -> Bool {
		calendar.source.kind == .iCloud && calendar.allowsModifications
	}

	/// The event written for a time box, searched in the four years around
	/// now (EventKit's widest range for one query).
	private func timeBoxEvent(_ id: String, in target: CalendarInfo) -> StoredEvent? {
		let now = Date()
		let twoYears: TimeInterval = 2 * 365 * 86400 - 86400
		let url = Self.timeBoxURL(id)
		return store.events(from: now.addingTimeInterval(-twoYears), to: now.addingTimeInterval(twoYears), calendarIds: [target.id])
			.first { $0.url == url }
	}

	private var calendar: Calendar {
		var calendar = Calendar(identifier: .gregorian)
		calendar.timeZone = timeZone
		return calendar
	}

	/// Minutes since midnight of a `YYYY-MM-DDTHH:MM:SS` civil time.
	private func minuteOfDay(_ civil: String) -> Int? {
		let clock = civil.split(separator: "T").last?.split(separator: ":") ?? []
		guard clock.count >= 2, let hour = Int(clock[0]), let minute = Int(clock[1]) else { return nil }
		return hour * 60 + minute
	}

	private func formatter(_ format: String) -> DateFormatter {
		let formatter = DateFormatter()
		formatter.calendar = calendar
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.timeZone = timeZone
		formatter.dateFormat = format
		return formatter
	}

	private func startOfDay(_ text: String) -> Date? {
		formatter("yyyy-MM-dd").date(from: text).map(calendar.startOfDay(for:))
	}

	private func civilText(_ date: Date) -> String {
		formatter("yyyy-MM-dd'T'HH:mm:ss").string(from: date)
	}

	private func civilDate(_ text: String) -> Date? {
		formatter("yyyy-MM-dd'T'HH:mm:ss").date(from: text)
	}
}
