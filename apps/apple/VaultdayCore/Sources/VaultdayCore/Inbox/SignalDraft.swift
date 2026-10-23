import Foundation

/// Which popover an outcome opens on a signal card.
public enum SignalDraftKind: Equatable, Sendable {
	/// + Task: a line under `## Next Steps`.
	case task
	/// Wait for the sender: a line under `## Pending Input`.
	case waitFor
	/// Attach to a story, without a task.
	case attach
	/// Snooze until a date.
	case snooze
}

/// An open outcome popover: its fields pre-filled from the mail's plain
/// fields only (the headline, the sender, the ticket number of a story the
/// vault holds). The day and the estimate start empty: nothing is guessed.
/// Save writes, Esc cancels.
public struct SignalDraft: Equatable, Sendable {
	public let signalId: String
	public let kind: SignalDraftKind
	public var text: String
	/// The story number; empty files the line into the mail intake note.
	public var story: String
	/// Who a Wait for line waits on.
	public var assignedTo: String
	/// `today`, `tomorrow` or `YYYY-MM-DD`; empty leaves the task unplanned.
	public var day: String
	/// `45m`, `2h`; empty leaves it unestimated.
	public var estimate: String
	/// When a snooze ends.
	public var snoozeUntil: Date

	public init(
		signalId: String,
		kind: SignalDraftKind,
		text: String,
		story: String,
		assignedTo: String,
		day: String,
		estimate: String,
		snoozeUntil: Date,
	) {
		self.signalId = signalId
		self.kind = kind
		self.text = text
		self.story = story
		self.assignedTo = assignedTo
		self.day = day
		self.estimate = estimate
		self.snoozeUntil = snoozeUntil
	}

	/// The popover for `kind` on `card`, as it opens. A snooze defaults to
	/// tomorrow at 08:00, a fixed default rather than a guess.
	public static func prefill(_ kind: SignalDraftKind, card: SignalCard, now: Date, calendar: Calendar) -> SignalDraft {
		SignalDraft(
			signalId: card.id,
			kind: kind,
			text: card.headline,
			story: card.hasMatchedStory ? card.ticket ?? "" : "",
			assignedTo: kind == .waitFor ? card.sender : "",
			day: "",
			estimate: "",
			snoozeUntil: defaultSnooze(after: now, calendar: calendar),
		)
	}

	/// The section heading the line goes under, for the popover's fixed row.
	public var sectionLabel: String? {
		switch kind {
		case .task: "## Next Steps"
		case .waitFor: "## Pending Input"
		case .attach, .snooze: nil
		}
	}

	/// The popover's title.
	public var title: String {
		switch kind {
		case .task: "New task"
		case .waitFor: "Wait for"
		case .attach: "Attach to story"
		case .snooze: "Snooze until"
		}
	}

	static func defaultSnooze(after now: Date, calendar: Calendar) -> Date {
		let tomorrow = calendar.date(byAdding: .day, value: 1, to: calendar.startOfDay(for: now)) ?? now
		return calendar.date(bySettingHour: 8, minute: 0, second: 0, of: tomorrow) ?? tomorrow
	}

	/// `day` as `YYYY-MM-DD`: `today` and `tomorrow` resolved in `calendar`;
	/// `nil` for empty, `.failure` for anything else unreadable.
	func resolvedDay(now: Date, calendar: Calendar) -> Result<String?, SignalDraftError> {
		let text = day.trimmingCharacters(in: .whitespaces).lowercased()
		switch text {
		case "":
			return .success(nil)
		case "today":
			return .success(SignalDates.day(now, calendar: calendar))
		case "tomorrow":
			let tomorrow = calendar.date(byAdding: .day, value: 1, to: now) ?? now
			return .success(SignalDates.day(tomorrow, calendar: calendar))
		default:
			let isDay = text.count == 10 && text.split(separator: "-").map(\.count) == [4, 2, 2]
			return isDay ? .success(text) : .failure(.unreadableDay(day))
		}
	}
}

/// Why Save did not write.
public enum SignalDraftError: Error, Equatable, Sendable {
	case emptyText
	case emptyStory
	case unreadableDay(String)

	public var message: String {
		switch self {
		case .emptyText: "Give the task a text"
		case .emptyStory: "Pick a story to attach it to"
		case let .unreadableDay(day): "\(day) is not a day; write today, tomorrow or 2026-10-30"
		}
	}
}
