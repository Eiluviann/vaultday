import Foundation

/// A run of the note's text with one style, as the story screen draws it.
public struct StorySpan: Hashable, Sendable {
	/// Where a span links to.
	public enum Link: Hashable, Sendable {
		/// A markdown link.
		case url(URL)
		/// A wikilink; `resolvedPath` is `nil` when no note of the vault matches.
		case note(target: String, section: String?, resolvedPath: String?, obsidianLink: URL?)
	}

	public let text: String
	public var isStrong = false
	public var isEmphasis = false
	public var isStrikethrough = false
	public var isHighlight = false
	public var isCode = false
	public var link: Link?

	public init(text: String, isStrong: Bool = false, isEmphasis: Bool = false, isStrikethrough: Bool = false, isHighlight: Bool = false, isCode: Bool = false, link: Link? = nil) {
		self.text = text
		self.isStrong = isStrong
		self.isEmphasis = isEmphasis
		self.isStrikethrough = isStrikethrough
		self.isHighlight = isHighlight
		self.isCode = isCode
		self.link = link
	}
}

/// What an embed shows.
public enum StoryEmbedKind: Hashable, Sendable {
	case image
	case note
	/// A file a plugin draws (`Excalidraw`, `Canvas`): a placeholder with a link.
	case plugin(String)
	case file
}

/// A view the app draws in place of a part of the note.
public enum StoryAppView: Hashable, Sendable {
	case nextSteps
	case pendingInput
	case activity
}

/// How a callout opens.
public enum StoryCalloutFold: Hashable, Sendable {
	case fixed
	case expanded
	case collapsed
}

/// A table column's alignment.
public enum StoryColumnAlignment: Hashable, Sendable {
	case natural
	case leading
	case center
	case trailing
}

/// The checkbox of a task line in the body.
public struct StoryCheckbox: Hashable, Sendable {
	public let state: BrowseRow.State
	/// The id the core writer changes the line by.
	public let taskId: String?

	public init(state: BrowseRow.State, taskId: String?) {
		self.state = state
		self.taskId = taskId
	}

	/// A step's checkbox: its state and the id the writer needs.
	public init(step: StoryStep) {
		self.init(state: step.state, taskId: step.taskId)
	}

	/// A line without an `[id:: ]` is shown but cannot be ticked from the app.
	public var isTickable: Bool { taskId != nil }
	public var isDone: Bool { state == .done || state == .cancelled }
}

/// An embed on a line of its own.
public struct StoryEmbed: Hashable, Sendable {
	public let kind: StoryEmbedKind
	public let target: String
	public let section: String?
	/// Relative to the workspace root, when the target resolves.
	public let resolvedPath: String?
	/// The file to load an image from.
	public let fileURL: URL?
	public let obsidianLink: URL?

	/// "Checkout flow.excalidraw", "Research notes > Sticky".
	public var label: String {
		section.map { "\(target) > \($0)" } ?? target
	}
}

/// One block of the rendered note, with the blocks nested in it.
public struct StoryBlock: Identifiable, Hashable, Sendable {
	public enum Kind: Hashable, Sendable {
		case heading(level: Int, title: String, spans: [StorySpan], obsidianLink: URL?)
		case paragraph([StorySpan])
		/// A list; its items are the children.
		case list(isOrdered: Bool)
		/// A list item; the items nested under it are the children.
		case listItem(marker: String, spans: [StorySpan], checkbox: StoryCheckbox?)
		case quote
		case callout(kind: String, title: [StorySpan], fold: StoryCalloutFold)
		case code(language: String?, text: String)
		case table(alignments: [StoryColumnAlignment], header: [[StorySpan]], rows: [[[StorySpan]]])
		case embed(StoryEmbed)
		/// A query Obsidian runs; `view` is the app's own view standing for it.
		case query(language: String, source: String, view: StoryAppView?)
		case appView(StoryAppView)
		case rule
	}

	/// The node's place in the core's flat list, unique in the note.
	public let id: Int
	/// The block's first line in the note, from 0.
	public let line: Int
	public let kind: Kind
	public var children: [StoryBlock]
}

/// A Next Steps or Pending Input line, in the note's order.
public struct StoryStep: Identifiable, Hashable, Sendable {
	public let id: String
	public let taskId: String?
	public let text: String
	public let state: BrowseRow.State
	public let depth: Int
	/// `YYYY-MM-DD` as written.
	public let scheduled: String?
	public let due: String?
	public let estimateMinutes: Int?
	public let completion: String?
	public let assignedTo: String?

	public var isDone: Bool { state == .done || state == .cancelled }
	public var estimateLabel: String? { estimateMinutes.map(DurationText.minutes) }
	/// "14 Oct".
	public var completionLabel: String? { completion.map(PreviewMapping.shortDay) }
	/// "28 Oct".
	public var dueLabel: String? { due.map(PreviewMapping.shortDay) }
}

/// Whose words the note is, from its provenance markers.
public struct StoryAuthorship: Hashable, Sendable {
	public enum Kind: Hashable, Sendable {
		case human
		case model
		case mixed
	}

	public let kind: Kind
	public let modelUnits: Int
	/// `YYYY-MM-DD`.
	public let latestModelDate: String?

	/// "Human", "Model-written · 19 Oct", "Mixed · model text 20 Oct".
	public var label: String {
		let day = latestModelDate.map(PreviewMapping.shortDay)
		switch kind {
		case .human: return "Human"
		case .model: return ["Model-written", day].compactMap(\.self).joined(separator: " · ")
		case .mixed: return ["Mixed", day.map { "model text \($0)" }].compactMap(\.self).joined(separator: " · ")
		}
	}

	/// Whether any of the note is model text, which carries the sparkle.
	public var hasModelText: Bool { kind != .human }
}

/// One label-and-value row of the Story block.
public struct StoryFact: Identifiable, Hashable, Sendable {
	public let label: String
	public let value: String
	/// Set on the Authorship row when model text is in the note.
	public var isModelText = false

	public var id: String { label }
}

/// Everything the story screen shows. Plain values; the views draw them.
public struct StoryContent: Hashable, Sendable {
	public let relativePath: String
	public let number: String
	public let title: String?
	public let sourceKind: String?
	public let sourceHost: String?
	public let status: String?
	public let priority: String?
	/// `YYYY-MM-DD`.
	public let due: String?
	public let estimate: String?
	public let project: String?
	public let authorship: StoryAuthorship
	public let blocks: [StoryBlock]
	public let nextSteps: [StoryStep]
	public let pendingInput: [StoryStep]
	public let waiting: [PreviewWaiting]
	public let activity: [PreviewActivity]
	public let obsidianLink: URL?

	/// "WEB-142 · issue · tracker.example.test".
	public var numberLine: String {
		[number, sourceKind, sourceHost].compactMap(\.self).joined(separator: " · ")
	}

	/// The Story block: status, project, priority, due date and authorship.
	public var facts: [StoryFact] {
		[
			StoryFact(label: "Status", value: status.map { $0.replacingOccurrences(of: "-", with: " ").capitalizedFirst } ?? Self.unset),
			StoryFact(label: "Project", value: project ?? Self.unset),
			StoryFact(label: "Priority", value: priority.map(\.capitalizedFirst) ?? Self.unset),
			StoryFact(label: "Due", value: due.map(PreviewMapping.shortDay) ?? Self.unset),
			StoryFact(label: "Authorship", value: authorship.label, isModelText: authorship.hasModelText),
		]
	}

	/// Whether the note has its own Activity query, where the list then shows.
	public var hasActivityInBody: Bool {
		StoryBlock.contains(blocks) { block in
			if case .query(_, _, .activity) = block.kind { return true }
			return false
		}
	}

	/// The section heading links, by title, for "Edit in Obsidian" at a section.
	public var sectionLinks: [String: URL] {
		var links: [String: URL] = [:]
		StoryBlock.visit(blocks) { block in
			if case let .heading(_, title, _, link?) = block.kind, links[title] == nil { links[title] = link }
		}
		return links
	}

	static let unset = "Not set"
}

extension StoryBlock {
	/// Whether any block of the tree satisfies `predicate`.
	static func contains(_ blocks: [StoryBlock], where predicate: (StoryBlock) -> Bool) -> Bool {
		blocks.contains { predicate($0) || contains($0.children, where: predicate) }
	}

	/// Calls `body` on every block of the tree, parents first.
	static func visit(_ blocks: [StoryBlock], _ body: (StoryBlock) -> Void) {
		for block in blocks {
			body(block)
			visit(block.children, body)
		}
	}
}
