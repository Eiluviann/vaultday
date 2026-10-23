import Foundation

/// How a workspace bills its time, as far as a report cares.
public enum ReportBilling: String, Sendable {
	case hourly
	case fixed
	case fte
}

/// Minutes for one story; `story` is `nil` for time that names none.
public struct ReportStoryMinutes: Equatable, Identifiable, Sendable {
	public let story: String?
	public let minutes: Int

	public var id: String { story ?? "none" }
	public var title: String { story ?? "No story" }
	public var text: String { DurationText.minutes(minutes) }

	public init(story: String?, minutes: Int) {
		self.story = story
		self.minutes = minutes
	}
}

/// One task of a fixed-price report: the quote against what was delivered.
public struct ReportFixedTask: Equatable, Identifiable, Sendable {
	public let story: String?
	public let task: String?
	public let text: String?
	public let quotedMinutes: Int?
	public let deliveredMinutes: Int
	public let overrunMinutes: Int

	public var id: String { (story ?? "") + "/" + (task ?? "none") }
	public var title: String { text ?? task ?? "No task" }

	/// "Quoted 2h · delivered 1h 15m", "No quote · delivered 20m", with the overrun when there is one.
	public var summary: String {
		let quote = quotedMinutes.map { "Quoted \(DurationText.minutes($0))" } ?? "No quote"
		let overrun = overrunMinutes > 0 ? " · \(DurationText.minutes(overrunMinutes)) unpaid" : ""
		return "\(quote) · delivered \(DurationText.minutes(deliveredMinutes))\(overrun)"
	}

	public init(story: String?, task: String?, text: String?, quotedMinutes: Int?, deliveredMinutes: Int, overrunMinutes: Int) {
		self.story = story
		self.task = task
		self.text = text
		self.quotedMinutes = quotedMinutes
		self.deliveredMinutes = deliveredMinutes
		self.overrunMinutes = overrunMinutes
	}
}

/// The hours section, by billing model. Tracked time is no case and no field:
/// it is the person's alone, and a report holds what was logged.
public enum ReportHours: Equatable, Sendable {
	/// No billing model: what was logged.
	case logged(minutes: Int, byStory: [ReportStoryMinutes])
	/// FTE: what was logged, nothing billed and no money.
	case fte(minutes: Int, byStory: [ReportStoryMinutes])
	/// Hourly: billable time by story, non-billable apart.
	case hourly(roundingMinutes: Int, billableMinutes: Int, billable: [ReportStoryMinutes], nonBillableMinutes: Int, nonBillable: [ReportStoryMinutes])
	/// Fixed price: quoted against delivered per task.
	case fixed(quotedMinutes: Int, deliveredMinutes: Int, overrunMinutes: Int, tasks: [ReportFixedTask])

	/// Whether the section has a money part. Only a billing model with rates
	/// could, and rates never reach the vault, so none does.
	public var hasMoneySection: Bool { false }
}

/// One day-log line of a report.
public struct ReportLogLine: Equatable, Identifiable, Sendable {
	public let date: String
	public let time: String
	public let text: String
	public let references: [String]
	public let agent: String?

	public var id: String { "\(date) \(time) \(text)" }

	public init(date: String, time: String, text: String, references: [String], agent: String?) {
		self.date = date
		self.time = time
		self.text = text
		self.references = references
		self.agent = agent
	}
}

/// Log lines of one kind.
public struct ReportLogGroup: Equatable, Identifiable, Sendable {
	/// `nil` for lines without a kind.
	public let kind: String?
	public let lines: [ReportLogLine]

	public var id: String { kind ?? "none" }
	public var title: String { kind.map { $0.prefix(1).uppercased() + $0.dropFirst() } ?? "No kind" }

	public init(kind: String?, lines: [ReportLogLine]) {
		self.kind = kind
		self.lines = lines
	}
}

/// Tasks of a section by state.
public struct ReportTaskCounts: Equatable, Sendable {
	public let total: Int
	public let open: Int
	public let inProgress: Int
	public let done: Int

	/// "4 · 3 open · 1 in progress", "None".
	public var text: String {
		guard total > 0 else { return "None" }
		let parts = [(open, "open"), (inProgress, "in progress"), (done, "done")]
			.filter { $0.0 > 0 }
			.map { "\($0.0) \($0.1)" }
		return ([String(total)] + parts).joined(separator: " · ")
	}

	public init(total: Int, open: Int, inProgress: Int, done: Int) {
		self.total = total
		self.open = open
		self.inProgress = inProgress
		self.done = done
	}
}

/// One story of a report.
public struct ReportStory: Equatable, Identifiable, Sendable {
	public let number: String?
	public let title: String?
	public let status: String?
	public let relativePath: String
	public let nextSteps: ReportTaskCounts
	public let pendingInput: ReportTaskCounts

	public var id: String { relativePath }
	public var heading: String { [number, title].compactMap(\.self).joined(separator: " · ") }

	public init(number: String?, title: String?, status: String?, relativePath: String, nextSteps: ReportTaskCounts, pendingInput: ReportTaskCounts) {
		self.number = number
		self.title = title
		self.status = status
		self.relativePath = relativePath
		self.nextSteps = nextSteps
		self.pendingInput = pendingInput
	}
}

/// A report as the Reports tab shows it, and the two texts it copies.
public struct ReportBundle: Equatable, Sendable {
	public let subject: String
	public let isClient: Bool
	public let range: ReportRange
	public let billing: ReportBilling?
	public let stories: [ReportStory]
	public let logGroups: [ReportLogGroup]
	public let sessionCount: Int
	public let unmeasuredSessionCount: Int
	public let hours: ReportHours
	/// The canonical JSON the core assembled; its `authorship` is empty for the CLI to fill.
	public let json: String
	public let markdown: String

	public var logLineCount: Int { logGroups.reduce(0) { $0 + $1.lines.count } }

	/// The authorship summary is the CLI's: the app shows where it is computed.
	public static let authorshipText = "Authorship: computed by the CLI"

	public init(
		subject: String, isClient: Bool, range: ReportRange, billing: ReportBilling?, stories: [ReportStory], logGroups: [ReportLogGroup],
		sessionCount: Int, unmeasuredSessionCount: Int, hours: ReportHours, json: String, markdown: String,
	) {
		self.subject = subject
		self.isClient = isClient
		self.range = range
		self.billing = billing
		self.stories = stories
		self.logGroups = logGroups
		self.sessionCount = sessionCount
		self.unmeasuredSessionCount = unmeasuredSessionCount
		self.hours = hours
		self.json = json
		self.markdown = markdown
	}
}

/// A report file just written.
public struct WrittenReport: Equatable, Sendable {
	public let relativePath: String
	public let absolutePath: String

	public init(relativePath: String, absolutePath: String) {
		self.relativePath = relativePath
		self.absolutePath = absolutePath
	}
}
