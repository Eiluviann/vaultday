import Foundation

/// Where a signal stands, as the Inbox shows it.
public enum SignalCardState: Equatable, Sendable {
	case new
	case snoozed(until: Date?)
	case dismissed
	case rejected
	case attached(story: String)
	case converted(taskId: String?)

	/// Whether the signal still waits for a decision: new, or snoozed.
	public var isInTriage: Bool {
		switch self {
		case .new, .snoozed: true
		case .dismissed, .rejected, .attached, .converted: false
		}
	}
}

/// One signal card, from the core's plain fields only: the source icon and
/// the sender on the first line with the time alone on the right, the
/// headline below, a tag row with the ticket number `ticket_pattern` found.
/// Nothing on it is guessed.
public struct SignalCard: Identifiable, Equatable, Sendable {
	public let id: String
	/// Where the signal came from: one icon shape and one colour per source;
	/// mail is the only source in the base version.
	public let source: SourceMark
	/// The SF Symbol of the source.
	public var sourceSymbol: String { source.mark.symbol }
	/// What the source icon is called, for VoiceOver.
	public var sourceName: String { source.mark.name }
	/// The sender's name, else the address.
	public let sender: String
	public let senderAddress: String
	public let receivedAt: Date?
	/// `09:42` for today, `22 Oct` for an earlier day.
	public let timeLabel: String
	/// The subject cleaned for a task line, as the card's headline.
	public let headline: String
	/// The subject as it arrived.
	public let subject: String
	/// The excerpt, shown in the preview only.
	public let excerpt: String
	/// The ticket number found in the subject or the excerpt.
	public let ticket: String?
	/// The title of the story with that number, when the vault holds one.
	public let ticketStoryTitle: String?
	public let state: SignalCardState

	public init(
		id: String,
		source: SourceMark = .mail,
		sender: String,
		senderAddress: String,
		receivedAt: Date?,
		timeLabel: String,
		headline: String,
		subject: String,
		excerpt: String,
		ticket: String?,
		ticketStoryTitle: String?,
		state: SignalCardState,
	) {
		self.id = id
		self.source = source
		self.sender = sender
		self.senderAddress = senderAddress
		self.receivedAt = receivedAt
		self.timeLabel = timeLabel
		self.headline = headline
		self.subject = subject
		self.excerpt = excerpt
		self.ticket = ticket
		self.ticketStoryTitle = ticketStoryTitle
		self.state = state
	}

	/// Whether the ticket names a story of this workspace, so Attach and
	/// + Task can file into it.
	public var hasMatchedStory: Bool { ticket != nil && ticketStoryTitle != nil }

	/// The Wait for button: the sender's first name, else the address's local part; bare when the sender is hidden.
	public var waitForLabel: String {
		let name = Self.shortName(of: sender)
		return name.isEmpty ? "Wait for" : "Wait for \(name)"
	}

	static func shortName(of sender: String) -> String {
		if let at = sender.firstIndex(of: "@") {
			return String(sender[..<at])
		}
		return sender.split(separator: " ").first.map(String.init) ?? sender
	}
}

extension SignalCard {
	/// Maps the core's view of a signal into a card, with the time read in `calendar`.
	static func from(_ view: InboxSignalView, now: Date, calendar: Calendar) -> SignalCard {
		let received = SignalDates.parse(view.receivedAt)
		return SignalCard(
			id: view.id,
			sender: view.sender,
			senderAddress: view.senderAddress,
			receivedAt: received,
			timeLabel: received.map { SignalDates.label(for: $0, now: now, calendar: calendar) } ?? "",
			headline: view.headline,
			subject: view.subject,
			excerpt: view.excerpt,
			ticket: view.ticket,
			ticketStoryTitle: view.ticketStoryTitle,
			state: state(of: view),
		)
	}

	private static func state(of view: InboxSignalView) -> SignalCardState {
		switch view.state {
		case .new: .new
		case .snoozed: .snoozed(until: view.snoozedUntil.flatMap(SignalDates.parse))
		case .dismissed: .dismissed
		case .rejected: .rejected
		case .attached: .attached(story: view.story ?? "")
		case .converted: .converted(taskId: view.task)
		}
	}
}

/// The RFC 3339 times the core hands over, and the short labels of the cards.
enum SignalDates {
	static func parse(_ text: String) -> Date? {
		let fractional = ISO8601DateFormatter()
		fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
		return fractional.date(from: text) ?? ISO8601DateFormatter().date(from: text)
	}

	static func rfc3339(_ date: Date) -> String {
		ISO8601DateFormatter().string(from: date)
	}

	/// `HH:mm` on the day of `now`, `d MMM` before it.
	static func label(for date: Date, now: Date, calendar: Calendar) -> String {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.timeZone = calendar.timeZone
		formatter.dateFormat = calendar.isDate(date, inSameDayAs: now) ? "HH:mm" : "d MMM"
		return formatter.string(from: date)
	}

	/// `YYYY-MM-DD` of `date` in `calendar`.
	static func day(_ date: Date, calendar: Calendar) -> String {
		let parts = calendar.dateComponents([.year, .month, .day], from: date)
		return String(format: "%04d-%02d-%02d", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0)
	}
}
