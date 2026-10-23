import Foundation
import Observation

/// What a drag carries between the panes: a task id or a signal id, told
/// apart by the `sig-` prefix the core gives every signal.
public enum DragPayload: Equatable, Sendable {
	case task(String)
	case signal(String)

	public init?(_ text: String) {
		let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
		guard !trimmed.isEmpty else { return nil }
		self = trimmed.hasPrefix("sig-") ? .signal(trimmed) : .task(trimmed)
	}
}

/// The Inbox column and the triage screen: the signals still in triage as
/// cards, the snoozed ones apart, the open outcome popover and the preview.
/// Every outcome goes through the core; only + Task and Wait for write to
/// the vault, one line each.
@MainActor
@Observable
public final class InboxModel {
	/// New signals, newest first: the cards.
	public private(set) var cards: [SignalCard] = []
	/// Snoozed signals, soonest back first.
	public private(set) var snoozed: [SignalCard] = []
	/// The open outcome popover.
	public var draft: SignalDraft?
	/// The signal shown in the preview slide-over.
	public private(set) var previewId: String?
	/// Why the last outcome did not run, or what it wrote.
	public private(set) var message: String?
	/// The stories Attach offers.
	public private(set) var stories: [StoryChoice] = []
	/// Whether a store is attached (a workspace is open).
	public private(set) var isOpen = false

	/// Called after a line was written, so Today and Browse read the vault again.
	@ObservationIgnored public var onPlanChanged: () -> Void = {}
	/// Called whenever the cards change, so the focus model knows their count.
	@ObservationIgnored public var onCardsChanged: (Int) -> Void = { _ in }

	@ObservationIgnored private var store: (any SignalStore)?
	@ObservationIgnored private let now: () -> Date
	@ObservationIgnored private let calendar: Calendar

	public init(now: @escaping () -> Date = Date.init, calendar: Calendar = .current) {
		self.now = now
		self.calendar = calendar
	}

	/// The switcher badge: signals new right now.
	public var newCount: Int { cards.count }

	/// The card shown in the preview.
	public var previewCard: SignalCard? {
		previewId.flatMap(card(id:))
	}

	public func card(at index: Int?) -> SignalCard? {
		guard let index, cards.indices.contains(index) else { return nil }
		return cards[index]
	}

	public func card(id: String) -> SignalCard? {
		cards.first { $0.id == id } ?? snoozed.first { $0.id == id }
	}

	/// Opens the Inbox over `store` and reads it.
	public func attach(store: any SignalStore) {
		self.store = store
		isOpen = true
		stories = store.storyChoices()
		reload()
	}

	/// The workspace closed.
	public func detach() {
		store = nil
		isOpen = false
		cards = []
		snoozed = []
		draft = nil
		previewId = nil
		stories = []
		onCardsChanged(0)
	}

	/// Reads the signals again: after a fetch, a switch or an outcome.
	public func reload() {
		guard let store else { return }
		do {
			let readAt = now()
			let all = try store.inboxSignals().map { SignalCard.from($0, now: readAt, calendar: calendar) }
			cards = all.filter { $0.state == .new }
			snoozed = all
				.filter { if case .snoozed = $0.state { true } else { false } }
				.sorted { Self.snoozeEnd(of: $0) < Self.snoozeEnd(of: $1) }
		} catch is SignalStoreClosed {
			// No single workspace is open (the All view): no cards, nothing to report.
			cards = []
			snoozed = []
		} catch {
			message = coreErrorMessage(error)
		}
		if let draft, card(id: draft.signalId)?.state.isInTriage != true { self.draft = nil }
		if let previewId, card(id: previewId) == nil { self.previewId = nil }
		onCardsChanged(cards.count)
	}

	// MARK: Outcomes

	/// Opens the popover of `kind` on a signal, pre-filled.
	public func begin(_ kind: SignalDraftKind, signalId: String) {
		guard let card = card(id: signalId) else { return }
		message = nil
		draft = SignalDraft.prefill(kind, card: card, now: now(), calendar: calendar)
	}

	/// Esc: closes the popover; nothing was written.
	public func cancelDraft() {
		draft = nil
	}

	/// Save: runs the open popover's outcome. `true` when it ran.
	@discardableResult
	public func saveDraft() -> Bool {
		guard let draft, let store else { return false }
		do {
			switch draft.kind {
			case .task, .waitFor:
				let converted = try store.convert(conversionInput(from: draft).get())
				message = "Wrote \(converted.taskId) to \(converted.note)"
				onPlanChanged()
			case .attach:
				let story = draft.story.trimmingCharacters(in: .whitespaces)
				guard !story.isEmpty else { throw SignalDraftError.emptyStory }
				try store.attach(signalId: draft.signalId, story: story)
				message = "Attached to \(story)"
			case .snooze:
				try store.snooze(signalId: draft.signalId, until: SignalDates.rfc3339(draft.snoozeUntil))
				message = nil
			}
			self.draft = nil
			reload()
			return true
		} catch let error as SignalDraftError {
			message = error.message
		} catch {
			message = coreErrorMessage(error)
		}
		return false
	}

	/// Dismiss: put away; nothing is written.
	public func dismiss(signalId: String) {
		run { try $0.dismiss(signalId: signalId) }
	}

	/// Not a signal: put away apart from a dismissal; nothing is written.
	public func markNotASignal(signalId: String) {
		run { try $0.reject(signalId: signalId) }
	}

	/// Reply in the mail app, through the signal's link.
	@discardableResult
	public func replyInMailApp(signalId: String) -> Bool {
		guard let store else { return false }
		let isOpened = store.openInMailApp(signalId: signalId)
		if !isOpened { message = "This message has no link to the mail app" }
		return isOpened
	}

	// MARK: Keys

	/// Runs a signal key on `signalId`: `1` + Task, `2` Wait for, `3` Attach,
	/// `4` Dismiss, `>>` Snooze. `false` for any other action.
	@discardableResult
	public func perform(_ action: KeyAction, on signalId: String?) -> Bool {
		guard let signalId, card(id: signalId) != nil else { return false }
		switch action {
		case .signalTask: begin(.task, signalId: signalId)
		case .signalWaitFor: begin(.waitFor, signalId: signalId)
		case .signalAttach: begin(.attach, signalId: signalId)
		case .signalSnooze: begin(.snooze, signalId: signalId)
		case .signalDismiss: dismiss(signalId: signalId)
		case .signalNotASignal: markNotASignal(signalId: signalId)
		case .signalReply: replyInMailApp(signalId: signalId)
		default: return false
		}
		return true
	}

	// MARK: Drops and preview

	/// A signal dropped into Today: a task line under Next Steps of its
	/// matched story (else the mail intake note), planned on `day`.
	@discardableResult
	public func dropIntoToday(signalId: String, day: String) -> Bool {
		guard let store, let card = card(id: signalId), card.state.isInTriage else { return false }
		var draft = SignalDraft.prefill(.task, card: card, now: now(), calendar: calendar)
		draft.day = day
		do {
			let converted = try store.convert(conversionInput(from: draft).get())
			message = "Wrote \(converted.taskId) to \(converted.note)"
			reload()
			onPlanChanged()
			return true
		} catch let error as SignalDraftError {
			message = error.message
		} catch {
			message = coreErrorMessage(error)
		}
		return false
	}

	/// A signal dropped onto a story: attached to it, no task.
	@discardableResult
	public func dropOnStory(signalId: String, story: String) -> Bool {
		guard let card = card(id: signalId), card.state.isInTriage else { return false }
		var isDone = false
		run {
			try $0.attach(signalId: signalId, story: story)
			isDone = true
		}
		if isDone { message = "Attached to \(story)" }
		return isDone
	}

	public func openPreview(signalId: String) {
		guard card(id: signalId) != nil else { return }
		previewId = signalId
	}

	public func closePreview() {
		previewId = nil
	}

	// MARK: Helpers

	private func run(_ outcome: (any SignalStore) throws -> Void) {
		guard let store else { return }
		do {
			try outcome(store)
			message = nil
		} catch {
			message = coreErrorMessage(error)
		}
		reload()
	}

	private func conversionInput(from draft: SignalDraft) -> Result<SignalConversionInput, SignalDraftError> {
		let text = draft.text.trimmingCharacters(in: .whitespaces)
		guard !text.isEmpty else { return .failure(.emptyText) }
		return draft.resolvedDay(now: now(), calendar: calendar).map { day in
			SignalConversionInput(
				signal: draft.signalId,
				outcome: draft.kind == .waitFor ? .waitFor : .task,
				text: text,
				story: Self.nonEmpty(draft.story),
				assignedTo: draft.kind == .waitFor ? Self.nonEmpty(draft.assignedTo) : nil,
				scheduled: day,
				estimate: Self.nonEmpty(draft.estimate),
			)
		}
	}

	private static func nonEmpty(_ text: String) -> String? {
		let trimmed = text.trimmingCharacters(in: .whitespaces)
		return trimmed.isEmpty ? nil : trimmed
	}

	private static func snoozeEnd(of card: SignalCard) -> Date {
		if case let .snoozed(until) = card.state { return until ?? .distantFuture }
		return .distantFuture
	}
}
