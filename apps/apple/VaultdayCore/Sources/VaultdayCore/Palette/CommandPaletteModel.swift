import Foundation
import Observation

/// A story the palette offers to jump to.
public struct PaletteStory: Hashable, Sendable {
	public let number: String
	public let title: String

	public init(number: String, title: String) {
		self.number = number
		self.title = title
	}
}

/// A workspace the palette offers to jump to, by name.
public struct PaletteWorkspace: Hashable, Sendable {
	public let id: String
	public let name: String

	public init(id: String, name: String) {
		self.id = id
		self.name = name
	}
}

/// What choosing a palette row does.
public enum PaletteChoice: Hashable, Sendable {
	case action(KeyAction)
	case story(String)
	case workspace(String)
}

/// One row of the palette: a command with its caps, a story or a workspace.
public struct PaletteRow: Identifiable, Hashable, Sendable {
	public let id: String
	public let title: String
	/// The caps of each sequence that runs the command; empty for a jump.
	public let caps: [[String]]
	/// The row's meta on the right: a story number.
	public let detail: String?
	public let choice: PaletteChoice
}

/// A titled run of rows.
public struct PaletteSection: Identifiable, Hashable, Sendable {
	public let title: String
	public let rows: [PaletteRow]

	public var id: String { title }
}

/// The command palette (⌘K, or `:` under Vim motions), rendered from the
/// same key table as the dispatcher and the footer: one row per action, its
/// label and caps from Keymap.yaml. The empty query shows the recent stories
/// and the workspaces first; typing filters every row by its words, so a
/// workspace is reached by its name.
@MainActor
@Observable
public final class CommandPaletteModel {
	public static let recentStoryLimit = 5
	public static let recentStoriesTitle = "Recent stories"
	public static let workspacesTitle = "Workspaces"

	public private(set) var isOpen = false
	public var query = "" {
		didSet { selectedIndex = 0 }
	}

	public private(set) var selectedIndex = 0
	public var recentStories: [PaletteStory] = []
	public var workspaces: [PaletteWorkspace] = []

	@ObservationIgnored private let dispatcher: KeyDispatcher

	public init(dispatcher: KeyDispatcher) {
		self.dispatcher = dispatcher
	}

	/// The key table of the current scheme as palette sections, in file order.
	public static func commandSections(_ table: [KeyTableRow]) -> [PaletteSection] {
		var titles: [String] = []
		var rowsByGroup: [String: [PaletteRow]] = [:]
		// The menu-bar extra's keys act in its popover only, never from here.
		for row in table where !row.action.isMenuBarAction {
			if rowsByGroup[row.group] == nil { titles.append(row.group) }
			rowsByGroup[row.group, default: []].append(
				PaletteRow(id: "action:\(row.action)", title: row.label, caps: row.caps, detail: nil, choice: .action(row.action)),
			)
		}
		return titles.map { PaletteSection(title: $0, rows: rowsByGroup[$0] ?? []) }
	}

	/// The sections the palette shows for the query now.
	public var sections: [PaletteSection] {
		let jumps = [
			PaletteSection(title: Self.recentStoriesTitle, rows: recentStories.prefix(Self.recentStoryLimit).map { story in
				PaletteRow(id: "story:\(story.number)", title: story.title, caps: [], detail: story.number, choice: .story(story.number))
			}),
			PaletteSection(title: Self.workspacesTitle, rows: workspaces.map { workspace in
				PaletteRow(id: "workspace:\(workspace.id)", title: "Go to \(workspace.name)", caps: [], detail: nil, choice: .workspace(workspace.id))
			}),
		]
		let all = jumps + Self.commandSections(dispatcher.keyTable)
		let words = query.lowercased().split(separator: " ").map(String.init)
		return all.compactMap { section in
			let rows = words.isEmpty ? section.rows : section.rows.filter { Self.matches($0, words: words) }
			return rows.isEmpty ? nil : PaletteSection(title: section.title, rows: rows)
		}
	}

	/// Every row shown, top to bottom, as the selection walks them.
	public var rows: [PaletteRow] { sections.flatMap(\.rows) }

	public var selectedRow: PaletteRow? {
		let rows = rows
		return rows.indices.contains(selectedIndex) ? rows[selectedIndex] : nil
	}

	public func open() {
		query = ""
		isOpen = true
	}

	public func close() {
		isOpen = false
		query = ""
	}

	/// Moves the highlighted row; it stays inside the list.
	public func moveSelection(by step: Int) {
		let count = rows.count
		guard count > 0 else { return }
		selectedIndex = min(max(selectedIndex + step, 0), count - 1)
	}

	/// Chooses the highlighted row, or `row`, and closes the palette.
	public func choose(_ row: PaletteRow? = nil) -> PaletteChoice? {
		guard let choice = (row ?? selectedRow)?.choice else { return nil }
		close()
		return choice
	}

	private static func matches(_ row: PaletteRow, words: [String]) -> Bool {
		let haystack = ([row.title, row.detail ?? ""] + row.caps.map { $0.joined() }).joined(separator: " ").lowercased()
		return words.allSatisfy { haystack.contains($0) }
	}
}
