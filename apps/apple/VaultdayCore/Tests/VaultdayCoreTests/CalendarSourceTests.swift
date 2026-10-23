import Foundation
import XCTest
@testable import VaultdayCore

/// The EventKit calendar source against a fake event store: where the
/// private calendar may live, what is refused, what reads cover.
final class CalendarSourceTests: XCTestCase {
	private let utc = TimeZone(identifier: "UTC") ?? .current
	private let iCloud = CalendarSourceInfo(id: "src-icloud", title: "iCloud", kind: .iCloud)
	private let exchange = CalendarSourceInfo(id: "src-exchange", title: "Work Exchange", kind: .exchange)

	private func date(_ text: String) -> Date {
		let formatter = ISO8601DateFormatter()
		formatter.timeZone = utc
		return formatter.date(from: text) ?? .distantPast
	}

	private func box(_ id: String = "box-1") -> CalendarTimeBox {
		CalendarTimeBox(id: id, start: "2026-10-22T13:30:00", durationMinutes: 45)
	}

	func testWithICloudCalendarOffThePrivateCalendarIsRefusedWithAMessage() {
		let store = FakeEventStore(sources: [exchange])
		store.calendarList = [CalendarInfo(id: "ex-vaultday", title: "Vaultday", source: exchange, allowsModifications: true)]
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: [], timeZone: utc)

		XCTAssertThrowsError(try source.ensurePrivateCalendar()) {
			XCTAssertEqual($0 as? CalendarRefusal, .iCloudCalendarOff)
			XCTAssertTrue(($0 as? CalendarRefusal)?.errorDescription?.contains("iCloud") == true)
		}
		XCTAssertThrowsError(try source.writeTimeBox(calendar: "Vaultday", title: "WEB-142", timeBox: box())) {
			guard case let .Unavailable(reason) = $0 as? CalendarFailure else { return XCTFail("\($0)") }
			XCTAssertTrue(reason.contains("iCloud Calendar is off"), reason)
		}
		XCTAssertEqual(store.created, [], "nothing is created outside iCloud")
		XCTAssertEqual(store.saved, [], "the Exchange calendar named Vaultday is never written")
	}

	func testThePrivateCalendarIsCreatedOnceAndOnlyInICloud() throws {
		let store = FakeEventStore(sources: [exchange, iCloud])
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: [], timeZone: utc)

		let calendar = try source.ensurePrivateCalendar()
		XCTAssertEqual(calendar.source, iCloud)
		XCTAssertEqual(calendar.title, "Vaultday")
		_ = try source.ensurePrivateCalendar()
		XCTAssertEqual(store.created.count, 1)
	}

	func testAReadOnlyOrLaterSharedCalendarOfThatNameIsNeverThePrivateOne() throws {
		let store = FakeEventStore(sources: [iCloud])
		store.calendarList = [CalendarInfo(id: "shared-ro", title: "Vaultday", source: iCloud, allowsModifications: false)]
		let record = InMemoryPrivateCalendarRecord()
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: [], record: record, timeZone: utc)

		let created = try source.ensurePrivateCalendar()
		XCTAssertNotEqual(created.id, "shared-ro", "a calendar without write access is not adopted")
		XCTAssertEqual(record.load(), created.id)

		// Someone shares a writable calendar of the same name later.
		store.calendarList.insert(CalendarInfo(id: "shared-rw", title: "Vaultday", source: iCloud, allowsModifications: true), at: 0)
		try source.writeTimeBox(calendar: "Vaultday", title: "WEB-142", timeBox: box())
		XCTAssertEqual(store.events.values.map(\.calendarId), [created.id], "the recorded calendar keeps its place")
	}

	func testACalendarAnotherMacCreatedIsAdoptedAndRecorded() throws {
		let store = FakeEventStore(sources: [iCloud])
		store.calendarList = [CalendarInfo(id: "from-mac-a", title: "Vaultday", source: iCloud, allowsModifications: true)]
		let record = InMemoryPrivateCalendarRecord()
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: [], record: record, timeZone: utc)
		XCTAssertEqual(try source.ensurePrivateCalendar().id, "from-mac-a")
		XCTAssertEqual(record.load(), "from-mac-a")
		XCTAssertEqual(store.created, [])
	}

	func testWithoutFullAccessEverythingIsRefused() {
		let store = FakeEventStore(sources: [iCloud], access: .writeOnly)
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: [], timeZone: utc)
		XCTAssertThrowsError(try source.ensurePrivateCalendar()) { XCTAssertEqual($0 as? CalendarRefusal, .accessNotGranted) }
		XCTAssertThrowsError(try source.events(from: "2026-10-22", to: "2026-10-22")) {
			guard case .Unavailable = $0 as? CalendarFailure else { return XCTFail("\($0)") }
		}
		XCTAssertThrowsError(try source.deleteTimeBox(calendar: "Vaultday", id: "box-1"))
	}

	func testTimeBoxesGoOnlyToThePrivateCalendar() throws {
		let store = FakeEventStore(sources: [iCloud, exchange])
		store.calendarList = [CalendarInfo(id: "ex-work", title: "Work", source: exchange, allowsModifications: true)]
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: ["ex-work"], timeZone: utc)

		XCTAssertThrowsError(try source.writeTimeBox(calendar: "Work", title: "WEB-142", timeBox: box())) {
			guard case .UnknownCalendar = $0 as? CalendarFailure else { return XCTFail("\($0)") }
		}
		try source.writeTimeBox(calendar: "Vaultday", title: "WEB-142 Checkout", timeBox: box())
		try source.writeTimeBox(calendar: "Vaultday", title: "WEB-142 Checkout, moved", timeBox: box())

		let saved = try XCTUnwrap(store.events.values.first)
		XCTAssertEqual(store.events.count, 1, "writing a box again replaces its event")
		XCTAssertEqual(saved.title, "WEB-142 Checkout, moved")
		XCTAssertEqual(saved.start, date("2026-10-22T13:30:00Z"))
		XCTAssertEqual(saved.end.timeIntervalSince(saved.start), 45 * 60)
		XCTAssertEqual(saved.url?.absoluteString, "vaultday://time-box/box-1")
		XCTAssertEqual(store.calendarList.first { $0.id == saved.calendarId }?.source, iCloud)

		try source.deleteTimeBox(calendar: "Vaultday", id: "box-1")
		XCTAssertTrue(store.events.isEmpty)
		XCTAssertThrowsError(try source.deleteTimeBox(calendar: "Vaultday", id: "box-1")) {
			guard case .TimeBoxNotFound = $0 as? CalendarFailure else { return XCTFail("\($0)") }
		}
		XCTAssertThrowsError(try source.deleteTimeBox(calendar: "Work", id: "box-1")) {
			guard case .UnknownCalendar = $0 as? CalendarFailure else { return XCTFail("\($0)") }
		}
	}

	func testReadsCoverOnlyThePickedCalendarsAndSkipAllDayEvents() throws {
		let store = FakeEventStore(sources: [iCloud, exchange])
		store.calendarList = [
			CalendarInfo(id: "work", title: "Work", source: exchange, allowsModifications: true),
			CalendarInfo(id: "home", title: "Home", source: iCloud, allowsModifications: true),
		]
		store.events = [
			"e1": StoredEvent(id: "e1", calendarId: "work", title: "Standup", start: date("2026-10-22T09:00:00Z"), end: date("2026-10-22T09:15:00Z"), isAllDay: false, url: nil),
			"e2": StoredEvent(id: "e2", calendarId: "work", title: "Holiday", start: date("2026-10-22T00:00:00Z"), end: date("2026-10-23T00:00:00Z"), isAllDay: true, url: nil),
			"e3": StoredEvent(id: "e3", calendarId: "home", title: "Dentist", start: date("2026-10-22T16:00:00Z"), end: date("2026-10-22T17:00:00Z"), isAllDay: false, url: nil),
		]
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: ["work"], timeZone: utc)

		let events = try source.events(from: "2026-10-22", to: "2026-10-22")
		XCTAssertEqual(events, [CalendarEvent(id: "e1", calendar: "Work", title: "Standup", start: "2026-10-22T09:00:00", end: "2026-10-22T09:15:00")])
		XCTAssertEqual(store.lastQueriedCalendars, ["work"])

		source.setPickedCalendars(["work", "home"])
		XCTAssertEqual(try source.events(from: "2026-10-22", to: "2026-10-22").map(\.id).sorted(), ["e1", "e3"])
		XCTAssertThrowsError(try source.events(from: "2026-10-23", to: "2026-10-22"))
	}

	func testThePrivateCalendarIsNotOfferedAsAPick() throws {
		let store = FakeEventStore(sources: [iCloud])
		let source = EventKitCalendarSource(store: store, pickedCalendarIds: [], timeZone: utc)
		_ = try source.ensurePrivateCalendar()
		store.calendarList.append(CalendarInfo(id: "home", title: "Home", source: iCloud, allowsModifications: true))
		XCTAssertEqual(source.pickableCalendars().map(\.title), ["Home"])
	}
}

private final class FakeEventStore: EventStoreAccess, @unchecked Sendable {
	var access: CalendarAccess
	var sourceList: [CalendarSourceInfo]
	var calendarList: [CalendarInfo] = []
	var events: [String: StoredEvent] = [:]
	var created: [CalendarInfo] = []
	var saved: [String] = []
	var lastQueriedCalendars: [String] = []
	private var nextId = 0

	init(sources: [CalendarSourceInfo], access: CalendarAccess = .fullAccess) {
		sourceList = sources
		self.access = access
	}

	func requestFullAccess() async throws -> Bool { access == .fullAccess }
	func sources() -> [CalendarSourceInfo] { sourceList }
	func calendars() -> [CalendarInfo] { calendarList }

	func createCalendar(title: String, in source: CalendarSourceInfo) throws -> CalendarInfo {
		let calendar = CalendarInfo(id: "cal-\(calendarList.count)", title: title, source: source, allowsModifications: true)
		calendarList.append(calendar)
		created.append(calendar)
		return calendar
	}

	func events(from start: Date, to end: Date, calendarIds: [String]) -> [StoredEvent] {
		lastQueriedCalendars = calendarIds
		return events.values.filter { calendarIds.contains($0.calendarId) && $0.start < end && $0.end > start }
	}

	func saveEvent(id: String?, calendarId: String, title: String, start: Date, end: Date, url: URL) throws -> String {
		let eventId = id ?? {
			nextId += 1
			return "ev-\(nextId)"
		}()
		events[eventId] = StoredEvent(id: eventId, calendarId: calendarId, title: title, start: start, end: end, isAllDay: false, url: url)
		saved.append(eventId)
		return eventId
	}

	func removeEvent(id: String) throws {
		events[id] = nil
	}
}
