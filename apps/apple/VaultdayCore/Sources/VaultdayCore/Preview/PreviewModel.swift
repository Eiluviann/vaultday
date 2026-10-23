import Foundation
import Observation

/// The task preview slide-over: the open task's story, note, subtasks,
/// waiting items and recent activity, read through the core. Read-only; the
/// buttons hand links to the app.
@MainActor
@Observable
public final class PreviewModel {
	public private(set) var errorMessage: String?
	/// Screen-share mode: the preview keeps the task and its facts, and leaves out the story note, the people waited on and the activity.
	public var isScreenShare = false

	private var loadedContent: TaskPreviewContent?

	private var vault: WorkspaceHandle?
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	/// What the preview shows: everything read, or the reduced form while screen-share mode is on.
	public var content: TaskPreviewContent? {
		isScreenShare ? loadedContent?.reducedForScreenShare : loadedContent
	}

	public var isOpen: Bool { loadedContent != nil }

	/// Points the model at the open workspace; an open preview closes.
	public func attach(to workspace: WorkspaceModel) {
		vault = workspace.vault
		close()
	}

	/// Opens the workspace at `path` without a grant (tests, and hosts outside the sandbox).
	public func attach(workspacePath path: String) {
		vault = try? openWorkspace(path: path)
		close()
	}

	/// Shows the preview of the task `taskId`.
	public func open(taskId: String) {
		guard let vault else { return }
		let today = DayText.string(now())
		do {
			loadedContent = PreviewMapping.content(try taskPreview(workspace: vault, id: taskId, today: today), today: today)
			errorMessage = nil
		} catch {
			loadedContent = nil
			errorMessage = coreErrorMessage(error)
		}
	}

	public func close() {
		loadedContent = nil
		errorMessage = nil
	}

	/// The link behind Open in Claude Code. The scheme is registered but nothing listens yet.
	public func claudeCodeLink(taskId: String) -> URL? {
		VaultdayLink.url(host: "claude-code", path: "/task/" + taskId)
	}

	/// The link behind Full story: `vaultday://story/<number>`, the story screen.
	public func fullStoryLink(storyNumber: String) -> URL? {
		VaultdayLink.url(host: "story", path: "/" + storyNumber)
	}
}

/// Reads the inline markdown a story note carries (bold, italics, code,
/// links) for display, and turns wikilinks into their visible text.
public enum PreviewText {
	public static func attributed(_ text: String) -> AttributedString {
		let plain = unwrapWikilinks(text)
		let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
		return (try? AttributedString(markdown: plain, options: options)) ?? AttributedString(plain)
	}

	/// `[[Brand kit]]` → `Brand kit`, `[[Brand kit|the kit]]` → `the kit`, `[[Note#Part]]` → `Note`.
	static func unwrapWikilinks(_ text: String) -> String {
		var result = ""
		var rest = Substring(text)
		while let open = rest.range(of: "[["), let close = rest[open.upperBound...].range(of: "]]") {
			result += rest[..<open.lowerBound]
			let inner = rest[open.upperBound ..< close.lowerBound]
			let shown = inner.split(separator: "|", maxSplits: 1).last.map(String.init) ?? String(inner)
			result += inner.contains("|") ? shown : String(shown.split(separator: "#", maxSplits: 1).first ?? Substring(shown))
			rest = rest[close.upperBound...]
		}
		return result + rest
	}
}
