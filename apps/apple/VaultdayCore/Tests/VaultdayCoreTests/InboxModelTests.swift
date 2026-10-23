import Foundation
import XCTest
@testable import VaultdayCore

/// The synthetic signals of `core/fixtures/signals/inbox.json`, as the core
/// hands them over: the fixture's `expected` values stand for what the core
/// pre-fills (the Rust tests check the core produces exactly these).
enum SignalFixture {
	static func views() throws -> [InboxSignalView] {
		let url = TestFixtures.repoRoot.appending(path: "core/fixtures/signals/inbox.json")
		let root = try JSONSerialization.jsonObject(with: Data(contentsOf: url))
		guard let object = root as? [String: Any], let signals = object["signals"] as? [[String: Any]] else {
			throw CocoaError(.fileReadCorruptFile)
		}
		let stories = ["WEB-142": "Checkout redesign", "ACME-12": "Invoice export"]
		return signals.enumerated().map { index, entry in
			let expected = entry["expected"] as? [String: Any] ?? [:]
			let ticket = expected["ticket"] as? String
			return InboxSignalView(
				id: String(format: "sig-fixture%09d", index),
				account: entry["account"] as? String ?? "",
				sender: expected["sender"] as? String ?? "",
				senderAddress: entry["sender_address"] as? String ?? "",
				receivedAt: entry["received_at"] as? String ?? "",
				subject: entry["subject"] as? String ?? "",
				headline: expected["headline"] as? String ?? "",
				excerpt: entry["excerpt"] as? String ?? "",
				ticket: ticket,
				ticketStoryTitle: ticket.flatMap { stories[$0] },
				state: .new,
				snoozedUntil: nil,
				task: nil,
				story: nil,
			)
		}
	}
}

extension InboxSignalView {
	/// The same signal in another state, as the core would hand it back.
	func with(state: SignalStateView, snoozedUntil: String? = nil, task: String? = nil, story: String? = nil) -> InboxSignalView {
		InboxSignalView(
			id: id, account: account, sender: sender, senderAddress: senderAddress, receivedAt: receivedAt,
			subject: subject, headline: headline, excerpt: excerpt, ticket: ticket, ticketStoryTitle: ticketStoryTitle,
			state: state, snoozedUntil: snoozedUntil, task: task, story: story,
		)
	}
}

/// A signal store over synthetic signals: it records every call and moves
/// the states as the core would.
@MainActor
final class FakeSignalStore: SignalStore {
	var signals: [InboxSignalView]
	private(set) var conversions: [SignalConversionInput] = []
	private(set) var attachments: [(signal: String, story: String)] = []
	private(set) var snoozes: [(signal: String, until: String)] = []
	private(set) var dismissals: [String] = []
	private(set) var rejections: [String] = []
	private(set) var opened: [String] = []
	private var nextTask = 100

	init(signals: [InboxSignalView]) {
		self.signals = signals
	}

	func inboxSignals() throws -> [InboxSignalView] { signals }

	func convert(_ input: SignalConversionInput) throws -> SignalConversionView {
		guard let index = signals.firstIndex(where: { $0.id == input.signal }), signals[index].state == .new || signals[index].state == .snoozed else {
			throw CoreError.Conflict(message: "\(input.signal) is already decided")
		}
		conversions.append(input)
		nextTask += 1
		let taskId = "task-\(nextTask)"
		signals[index] = signals[index].with(state: .converted, task: taskId)
		return SignalConversionView(taskId: taskId, note: input.story ?? "Inbox/Mail.md", line: "- [ ] \(input.text) [id:: \(taskId)]")
	}

	func attach(signalId: String, story: String) throws {
		attachments.append((signalId, story))
		set(signalId, .attached, story: story)
	}

	func snooze(signalId: String, until: String) throws {
		snoozes.append((signalId, until))
		set(signalId, .snoozed, until: until)
	}

	func dismiss(signalId: String) throws {
		dismissals.append(signalId)
		set(signalId, .dismissed)
	}

	func reject(signalId: String) throws {
		rejections.append(signalId)
		set(signalId, .rejected)
	}

	func storyChoices() -> [StoryChoice] {
		[StoryChoice(number: "WEB-142", title: "Checkout redesign"), StoryChoice(number: "ACME-12", title: "Invoice export")]
	}

	func openInMailApp(signalId: String) -> Bool {
		opened.append(signalId)
		return true
	}

	private func set(_ id: String, _ state: SignalStateView, until: String? = nil, story: String? = nil) {
		if let index = signals.firstIndex(where: { $0.id == id }) {
			signals[index] = signals[index].with(state: state, snoozedUntil: until, story: story)
		}
	}
}

@MainActor
final class InboxModelTests: XCTestCase {
	private static let utc: Calendar = {
		var calendar = Calendar(identifier: .gregorian)
		calendar.timeZone = TimeZone(identifier: "UTC")!
		return calendar
	}()

	/// Friday 23 October 2026, 10:00 UTC.
	private static let friday = ISO8601DateFormatter().date(from: "2026-10-23T10:00:00Z")!

	private func model() throws -> (InboxModel, FakeSignalStore) {
		let store = try FakeSignalStore(signals: SignalFixture.views())
		let inbox = InboxModel(now: { Self.friday }, calendar: Self.utc)
		inbox.attach(store: store)
		return (inbox, store)
	}

	// MARK: Card mapping

	func testCardsMapThePlainFieldsWithTheTimeAloneOnTheRight() throws {
		let (inbox, _) = try model()
		XCTAssertEqual(inbox.cards.map(\.sender), ["Lena Ortiz", "billing-bot@example.test", "Sam Patel"])
		let first = try XCTUnwrap(inbox.cards.first)
		XCTAssertEqual(first.sourceSymbol, "envelope")
		XCTAssertEqual(first.timeLabel, "07:41", "a time of today")
		XCTAssertEqual(first.headline, "WEB-142 checkout, two comments on the summary")
		XCTAssertEqual(first.ticket, "WEB-142")
		XCTAssertTrue(first.hasMatchedStory)
		XCTAssertEqual(first.waitForLabel, "Wait for Lena")
		XCTAssertEqual(inbox.cards[1].waitForLabel, "Wait for billing-bot")
		XCTAssertEqual(inbox.cards[2].timeLabel, "22 Oct", "an earlier day shows its date")
		XCTAssertNil(inbox.cards[2].ticket, "no ticket in the mail, no tag")
		XCTAssertFalse(inbox.cards[2].hasMatchedStory)
	}

	func testStatesMapAndOnlyNewSignalsAreCards() throws {
		var views = try SignalFixture.views()
		views[0] = views[0].with(state: .snoozed, snoozedUntil: "2026-10-24T08:00:00Z")
		views[1] = views[1].with(state: .attached, story: "ACME-12")
		let mapped = views.map { SignalCard.from($0, now: Self.friday, calendar: Self.utc) }
		XCTAssertEqual(mapped[0].state, .snoozed(until: ISO8601DateFormatter().date(from: "2026-10-24T08:00:00Z")))
		XCTAssertEqual(mapped[1].state, .attached(story: "ACME-12"))
		XCTAssertFalse(mapped[1].state.isInTriage)
		let store = FakeSignalStore(signals: views)
		let inbox = InboxModel(now: { Self.friday }, calendar: Self.utc)
		inbox.attach(store: store)
		XCTAssertEqual(inbox.cards.map(\.sender), ["Sam Patel"])
		XCTAssertEqual(inbox.snoozed.map(\.sender), ["Lena Ortiz"])
	}

	// MARK: Popover prefill

	func testPopoversArePrefilledFromThePlainFieldsOnly() throws {
		let (inbox, _) = try model()
		let lena = try XCTUnwrap(inbox.cards.first)
		inbox.begin(.task, signalId: lena.id)
		let task = try XCTUnwrap(inbox.draft)
		XCTAssertEqual(task.text, "WEB-142 checkout, two comments on the summary")
		XCTAssertEqual(task.story, "WEB-142", "the ticket of a story the vault holds")
		XCTAssertEqual(task.day, "", "no guessed day")
		XCTAssertEqual(task.estimate, "", "no guessed estimate")
		XCTAssertEqual(task.assignedTo, "")
		XCTAssertEqual(task.sectionLabel, "## Next Steps")
		XCTAssertEqual(task.subject, lena.subject)
		XCTAssertEqual(task.sourceLabel, lena.subject, "the header names the subject, never the sig- id")
		XCTAssertFalse(task.sourceLabel.hasPrefix("sig-"))
		XCTAssertEqual(SignalDraft(signalId: "sig-1", kind: .task, text: "", story: "", assignedTo: "", day: "", estimate: "", snoozeUntil: .distantFuture, subject: " ").sourceLabel, "the mail")
		inbox.begin(.waitFor, signalId: lena.id)
		XCTAssertEqual(inbox.draft?.assignedTo, "Lena Ortiz")
		XCTAssertEqual(inbox.draft?.sectionLabel, "## Pending Input")
		let sam = inbox.cards[2]
		inbox.begin(.attach, signalId: sam.id)
		XCTAssertEqual(inbox.draft?.story, "", "no ticket, no story")
		inbox.begin(.snooze, signalId: sam.id)
		XCTAssertEqual(inbox.draft?.snoozeUntil, ISO8601DateFormatter().date(from: "2026-10-24T08:00:00Z"), "a fixed default: tomorrow 08:00")
		inbox.cancelDraft()
		XCTAssertNil(inbox.draft, "Esc cancels")
	}

	func testSaveWritesThroughTheCoreWithTheEditedFields() throws {
		let (inbox, store) = try model()
		var planChanges = 0
		inbox.onPlanChanged = { planChanges += 1 }
		let lena = try XCTUnwrap(inbox.cards.first)
		inbox.begin(.task, signalId: lena.id)
		inbox.draft?.day = "today"
		inbox.draft?.estimate = "30m"
		XCTAssertTrue(inbox.saveDraft())
		let input = try XCTUnwrap(store.conversions.first)
		XCTAssertEqual(input.outcome, .task)
		XCTAssertEqual(input.text, "WEB-142 checkout, two comments on the summary")
		XCTAssertEqual(input.story, "WEB-142")
		XCTAssertEqual(input.scheduled, "2026-10-23")
		XCTAssertEqual(input.estimate, "30m")
		XCTAssertNil(input.assignedTo)
		XCTAssertNil(inbox.draft)
		XCTAssertEqual(planChanges, 1)
		XCTAssertFalse(inbox.cards.contains { $0.id == lena.id }, "an accepted signal leaves the cards")

		let billing = inbox.cards[0]
		inbox.begin(.waitFor, signalId: billing.id)
		inbox.draft?.text = "  "
		XCTAssertFalse(inbox.saveDraft(), "an empty text is refused before the core")
		XCTAssertEqual(inbox.message, "Give the task a text")
		inbox.draft?.text = "Export rerun"
		inbox.draft?.day = "someday"
		XCTAssertFalse(inbox.saveDraft())
		inbox.draft?.day = ""
		XCTAssertTrue(inbox.saveDraft())
		XCTAssertEqual(store.conversions.last?.outcome, .waitFor)
		XCTAssertEqual(store.conversions.last?.assignedTo, "billing-bot@example.test")
		XCTAssertEqual(store.conversions.last?.story, "ACME-12", "the matched story's number")
	}

	// MARK: Keyboard

	func testTriageKeysRunThroughTheDispatcherOnTheInboxPaneOnly() throws {
		let (inbox, store) = try model()
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		let focus = PaneFocusModel(scheme: .vim)
		let session = KeyboardSession(dispatcher: KeyDispatcher(keymap: keymap, scheme: .vim), focus: focus, register: PlanRegister(plan: FakePlan()) { _ in })
		inbox.onCardsChanged = { focus.setItemCount($0, in: .inbox) }
		focus.setItemCount(inbox.cards.count, in: .inbox)
		let selected = { inbox.card(at: focus.selection(in: .inbox))?.id }
		session.selectedSignalId = selected
		var otherActions: [KeyAction] = []
		session.onAction = { action in
			if !inbox.perform(action, on: selected()) { otherActions.append(action) }
		}
		func type(_ strokes: String) {
			for notation in strokes.split(separator: " ") {
				guard let stroke = KeyStroke(notation: String(notation)) else { return XCTFail("not a stroke: \(notation)") }
				session.handle(stroke)
			}
		}

		type("1")
		XCTAssertNil(inbox.draft, "the digits are the Inbox's only while it holds focus")
		type("g 2 j")
		XCTAssertEqual(focus.focusedPane, .inbox)
		XCTAssertEqual(selected(), inbox.cards[0].id)
		type("1")
		XCTAssertEqual(inbox.draft?.kind, .task)
		inbox.cancelDraft()
		type("j 2")
		XCTAssertEqual(inbox.draft?.kind, .waitFor)
		XCTAssertEqual(inbox.draft?.signalId, inbox.cards[1].id)
		inbox.cancelDraft()
		type("3")
		XCTAssertEqual(inbox.draft?.kind, .attach)
		inbox.cancelDraft()
		type("> >")
		XCTAssertEqual(inbox.draft?.kind, .snooze, ">> snoozes the signal, not a task")
		XCTAssertTrue(inbox.saveDraft())
		XCTAssertEqual(store.snoozes.first?.signal, "sig-fixture000000001")
		XCTAssertEqual(store.snoozes.first?.until, "2026-10-24T08:00:00Z")
		XCTAssertEqual(inbox.cards.count, 2, "the snoozed one left the cards")
		type("g g 4")
		XCTAssertEqual(store.dismissals, ["sig-fixture000000000"])
		type("\\")
		XCTAssertTrue(session.isItemMenuOpen, "\\ opens the signal's menu")
		XCTAssertTrue(session.showsItemMenu(in: .inbox))
		XCTAssertFalse(session.showsItemMenu(in: .today))
		type("escape")
		XCTAssertFalse(session.isItemMenuOpen)
		XCTAssertFalse(otherActions.contains(.pushToTomorrow), ">> never pushed a task from the Inbox")
	}

	func testTheSignalKeysAreLabelledAndScopedInTheKeymap() throws {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		for scheme in KeyScheme.allCases {
			let sequences = keymap.sequences(for: scheme)
			for action in [KeyAction.signalTask, .signalWaitFor, .signalAttach, .signalDismiss, .signalSnooze] {
				let bound = sequences.filter { $0.action == action }
				XCTAssertFalse(bound.isEmpty, "\(action) is bound under \(scheme)")
				XCTAssertTrue(bound.allSatisfy { $0.pane == .inbox }, "\(action) acts on the Inbox pane only")
			}
			let rows = keymap.table(for: scheme).filter(\.action.isSignalAction)
			XCTAssertEqual(rows.count, 7)
			XCTAssertTrue(rows.allSatisfy { !$0.label.isEmpty && !$0.label.hasSuffix(".") })
		}
	}

	// MARK: Badge

	func testTheBadgeCountsNewSignalsOnly() throws {
		let (inbox, store) = try model()
		XCTAssertEqual(inbox.newCount, 3)
		inbox.dismiss(signalId: inbox.cards[0].id)
		XCTAssertEqual(inbox.newCount, 2)
		inbox.begin(.snooze, signalId: inbox.cards[0].id)
		inbox.saveDraft()
		XCTAssertEqual(inbox.newCount, 1, "a snoozed signal is not counted")
		inbox.markNotASignal(signalId: inbox.cards[0].id)
		XCTAssertEqual(inbox.newCount, 0)
		XCTAssertEqual(store.rejections.count, 1)
		inbox.detach()
		XCTAssertEqual(inbox.newCount, 0)
		XCTAssertEqual(QuietSignalNotification.body(added: 1, waiting: 1), "1 new signal")
		XCTAssertEqual(QuietSignalNotification.body(added: 2, waiting: 5), "2 new signals · 5 to decide")
	}

	func testTheBadgeCountsNewSignalsWhileScreenShareSkipsTheNotification() throws {
		let screenShare = ScreenShareModel()
		XCTAssertTrue(screenShare.postsSignalNotifications)
		screenShare.restore(isOn: true, returnTo: nil)
		XCTAssertFalse(screenShare.postsSignalNotifications, "the quiet notification is skipped")
		let (inbox, _) = try model()
		XCTAssertEqual(inbox.newCount, 3, "the badge still counts every new signal")
		inbox.reload()
		XCTAssertEqual(inbox.newCount, 3)
	}

	// MARK: Drops

	func testDropPayloadsTellSignalsFromTasks() {
		XCTAssertEqual(DragPayload("sig-0123456789abcdef"), .signal("sig-0123456789abcdef"))
		XCTAssertEqual(DragPayload("task-5"), .task("task-5"))
		XCTAssertNil(DragPayload("  "))
	}

	func testASignalDroppedIntoTodayBecomesATaskAndOntoAStoryIsAttached() throws {
		let (inbox, store) = try model()
		var planChanges = 0
		inbox.onPlanChanged = { planChanges += 1 }
		let lena = inbox.cards[0]
		XCTAssertTrue(inbox.dropIntoToday(signalId: lena.id, day: "2026-10-23"))
		let input = try XCTUnwrap(store.conversions.first)
		XCTAssertEqual(input.outcome, .task)
		XCTAssertEqual(input.story, "WEB-142")
		XCTAssertEqual(input.scheduled, "2026-10-23")
		XCTAssertEqual(planChanges, 1)
		XCTAssertFalse(inbox.dropIntoToday(signalId: lena.id, day: "2026-10-23"), "converted once")
		XCTAssertEqual(store.conversions.count, 1)
		let sam = try XCTUnwrap(inbox.cards.last)
		XCTAssertTrue(inbox.dropOnStory(signalId: sam.id, story: "ACME-12"))
		XCTAssertEqual(store.attachments.first?.story, "ACME-12")
		XCTAssertFalse(inbox.dropOnStory(signalId: sam.id, story: "WEB-142"), "an attached signal is decided")
		XCTAssertEqual(inbox.newCount, 1)
	}

	func testReplyOpensTheMailAppThroughTheSignalLink() throws {
		let (inbox, store) = try model()
		XCTAssertTrue(inbox.perform(.signalReply, on: inbox.cards[0].id))
		XCTAssertEqual(store.opened, [inbox.cards[0].id])
		XCTAssertFalse(inbox.perform(.completeTask, on: inbox.cards[0].id), "a task key is not the Inbox's")
		XCTAssertFalse(inbox.perform(.signalTask, on: nil))
	}
}
