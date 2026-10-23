import Foundation

/// The colour a mark is drawn in, named by its design token; the app maps
/// each to `Tokens.Colors`. Marks keep to the hues' jobs: no workspace or
/// sunrise colour ever marks a source or a type.
public enum MarkTone: Equatable, Sendable {
	case inkSecondary
	case inkMuted
	case info
	case positive
	case caution
	case ai
}

/// One icon shape and one fixed colour, as a source or a type is always drawn.
public struct IconMark: Equatable, Sendable {
	/// The SF Symbol drawn; for a design-named custom symbol, the closest
	/// system symbol until the custom one is authored (``isCustomSymbolPending``).
	public let symbol: String
	public let tone: MarkTone
	/// What VoiceOver says.
	public let name: String
	/// The design names a custom symbol (Teams mark, lightning bolt, running
	/// figure, wand, screen-share, hand) that is still to be authored in the
	/// SF Symbols app; this mark uses the closest system symbol meanwhile.
	public let isCustomSymbolPending: Bool

	public init(symbol: String, tone: MarkTone, name: String, isCustomSymbolPending: Bool = false) {
		self.symbol = symbol
		self.tone = tone
		self.name = name
		self.isCustomSymbolPending = isCustomSymbolPending
	}
}

/// Where a signal or an entry came from.
public enum SourceMark: CaseIterable, Sendable {
	/// Mail read over IMAP.
	case mail
	/// Microsoft 365 mail (Outlook).
	case outlook
	/// Microsoft Teams (a later feature set).
	case teams
	/// Something captured by hand: quick add, a browser capture.
	case capture
	/// A Reminders item.
	case reminders
	/// A calendar event.
	case calendar

	public var mark: IconMark {
		switch self {
		case .mail: IconMark(symbol: "envelope", tone: .inkSecondary, name: "Mail")
		case .outlook: IconMark(symbol: "envelope.badge", tone: .info, name: "Outlook")
		// custom symbol pending: Teams mark
		case .teams: IconMark(symbol: "bubble.left.and.bubble.right", tone: .info, name: "Teams", isCustomSymbolPending: true)
		// custom symbol pending: lightning bolt
		case .capture: IconMark(symbol: "bolt", tone: .caution, name: "Captured", isCustomSymbolPending: true)
		case .reminders: IconMark(symbol: "checklist", tone: .inkSecondary, name: "Reminders")
		case .calendar: IconMark(symbol: "calendar", tone: .inkMuted, name: "Calendar")
		}
	}
}

/// What kind of thing a row or a log line is.
public enum TypeMark: CaseIterable, Sendable {
	case task
	case session
	case meeting
	case done
	case decision
	case modelDecision
	case blocker
	case status
	case note
	case chaser
	case privateTime
	case story
	case workspace
	case command

	public var mark: IconMark {
		switch self {
		case .task: IconMark(symbol: "checkmark.circle", tone: .inkMuted, name: "Task")
		case .session: IconMark(symbol: "clock", tone: .positive, name: "Session")
		case .meeting: IconMark(symbol: "person.2", tone: .positive, name: "Meeting")
		case .done: IconMark(symbol: "checkmark.seal", tone: .positive, name: "Done")
		case .decision: IconMark(symbol: "lightbulb", tone: .inkSecondary, name: "Decision")
		// custom symbol pending: wand
		case .modelDecision: IconMark(symbol: "sparkles", tone: .ai, name: "Decision by a model", isCustomSymbolPending: true)
		case .blocker: IconMark(symbol: "nosign", tone: .caution, name: "Blocker")
		case .status: IconMark(symbol: "arrow.right", tone: .inkSecondary, name: "Status")
		case .note: IconMark(symbol: "bookmark", tone: .inkMuted, name: "Note")
		// custom symbol pending: running figure
		case .chaser: IconMark(symbol: "figure.run", tone: .caution, name: "Chaser", isCustomSymbolPending: true)
		// custom symbol pending: hand
		case .privateTime: IconMark(symbol: "hand.raised", tone: .inkMuted, name: "Private, never in a report", isCustomSymbolPending: true)
		case .story: IconMark(symbol: "doc.text", tone: .inkMuted, name: "Story")
		case .workspace: IconMark(symbol: "square.stack", tone: .inkMuted, name: "Workspace")
		case .command: IconMark(symbol: "command", tone: .inkMuted, name: "Command")
		}
	}

	/// The mark of a palette row's kind.
	public init(paletteChoice choice: PaletteChoice) {
		switch choice {
		case .action: self = .command
		case .story: self = .story
		case .workspace: self = .workspace
		}
	}

	/// The mark of a day-log line's `[kind:: ]`, a line a model wrote
	/// (`[agent:: ]`) marked as such.
	public init(dayLogKind kind: String?, isByModel: Bool) {
		switch kind {
		case "done": self = .done
		case "session": self = .session
		case "meeting": self = .meeting
		case "decision": self = isByModel ? .modelDecision : .decision
		case "blocker": self = .blocker
		case "status": self = .status
		default: self = .note
		}
	}
}
