import Foundation

/// One task row of the Browse table, as the views see it.
public struct BrowseRow: Identifiable, Hashable, Sendable {
	public enum State: Hashable, Sendable {
		case open
		case inProgress
		case done
		case cancelled
	}

	/// The task id, unique in the workspace.
	public let id: String
	public let text: String
	public let state: State
	/// Who the task waits on, when it is assigned to someone but me.
	public let waitingOn: String?
	public let waitingDays: Int?
	/// `today`, `tomorrow`, `Thu` or `Oct 20`.
	public let scheduledLabel: String?
	/// `Oct 20`.
	public let dueLabel: String?
	public let isOverdue: Bool
	public let estimateMinutes: Int?
	public let storyNumber: String?

	public var estimateLabel: String? { estimateMinutes.map(DurationText.minutes) }
	public var isDone: Bool { state == .done || state == .cancelled }
}

/// A section of a story in the table: `Next Steps` or `Pending Input`.
public struct BrowseSectionGroup: Identifiable, Hashable, Sendable {
	public let id: String
	/// `nil` when the query does not group by section (`group:story`, `group:none`).
	public let title: String?
	public let rows: [BrowseRow]
}

/// A story with its matching tasks, one fold of the table.
public struct BrowseStoryGroup: Identifiable, Hashable, Sendable {
	public let id: String
	public let storyNumber: String?
	public let title: String
	public let taskCount: Int
	public let sections: [BrowseSectionGroup]

	/// "7 open", as the group header says it.
	public var countLabel: String { "\(taskCount) open" }
}

/// One selectable line of the table: a story's header or one of its rows.
public enum BrowseItem: Hashable, Sendable {
	case header(BrowseStoryGroup)
	case row(BrowseRow)

	public var id: String {
		switch self {
		case let .header(group): "story:" + group.id
		case let .row(row): "task:" + row.id
		}
	}
}

/// A saved query line with a name.
public struct BrowsePreset: Identifiable, Hashable, Codable, Sendable {
	public var id: String { name }
	public let name: String
	public let query: String
	/// Presets every workspace starts with come from the core and cannot be removed.
	public let isBuiltIn: Bool

	public init(name: String, query: String, isBuiltIn: Bool = false) {
		self.name = name
		self.query = query
		self.isBuiltIn = isBuiltIn
	}

	private enum CodingKeys: String, CodingKey { case name, query }

	public init(from decoder: any Decoder) throws {
		let container = try decoder.container(keyedBy: CodingKeys.self)
		name = try container.decode(String.self, forKey: .name)
		query = try container.decode(String.self, forKey: .query)
		isBuiltIn = false
	}

	public func encode(to encoder: any Encoder) throws {
		var container = encoder.container(keyedBy: CodingKeys.self)
		try container.encode(name, forKey: .name)
		try container.encode(query, forKey: .query)
	}
}

/// The Browse presets a user saved on this Mac, per workspace. They are kept
/// in the App Group container next to the window state, never in the
/// workspace folder; losing the file only forgets the saved presets.
public struct BrowsePresetStore: Sendable {
	public let file: URL?

	/// `file` is `nil` where no container exists (tests, unsigned runs): nothing is kept.
	public init(file: URL?) {
		self.file = file
	}

	public func load() -> [BrowsePreset] {
		guard let file, let data = try? Data(contentsOf: file),
		      let presets = try? JSONDecoder().decode([BrowsePreset].self, from: data)
		else { return [] }
		return presets
	}

	public func save(_ presets: [BrowsePreset]) {
		guard let file, let data = try? JSONEncoder().encode(presets.filter { !$0.isBuiltIn }) else { return }
		try? FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
		try? data.write(to: file, options: [.atomic])
	}
}

/// How the table writes a day.
enum BrowseDateText {
	private static let calendar = Calendar(identifier: .gregorian)

	/// `today`, `tomorrow`, `yesterday`, a weekday inside the next six days, else `Oct 20`.
	static func scheduled(_ iso: String, today: String) -> String {
		guard let day = DayText.date(from: iso), let now = DayText.date(from: today) else { return iso }
		let offset = calendar.dateComponents([.day], from: now, to: day).day ?? 0
		switch offset {
		case 0: return "today"
		case 1: return "tomorrow"
		case -1: return "yesterday"
		case 2 ... 6: return weekday.string(from: day)
		default: return short(day)
		}
	}

	/// `Oct 20`.
	static func due(_ iso: String) -> String {
		DayText.date(from: iso).map(short) ?? iso
	}

	private static func short(_ day: Date) -> String { monthDay.string(from: day) }

	private static let weekday = formatter("EEE")
	private static let monthDay = formatter("MMM d")

	private static func formatter(_ format: String) -> DateFormatter {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.calendar = calendar
		formatter.timeZone = .autoupdatingCurrent
		formatter.dateFormat = format
		return formatter
	}
}
