import Foundation
import Observation

/// The task preview slide-over: the open task's story, note, subtasks,
/// waiting items and recent activity, read through the core. Read-only; the
/// buttons hand links to the app.
@MainActor
@Observable
public final class PreviewModel {
	public private(set) var content: TaskPreviewContent?
	public private(set) var errorMessage: String?

	private var vault: WorkspaceHandle?
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	public var isOpen: Bool { content != nil }

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
			content = PreviewMapping.content(try taskPreview(workspace: vault, id: taskId, today: today), today: today)
			errorMessage = nil
		} catch {
			content = nil
			errorMessage = coreErrorMessage(error)
		}
	}

	public func close() {
		content = nil
		errorMessage = nil
	}

	/// The link behind Open in Claude Code. The scheme is registered but nothing listens yet.
	public func claudeCodeLink(taskId: String) -> URL? {
		VaultdayLink.url(host: "claude-code", path: "/task/" + taskId)
	}

	/// The link behind Full story; the story page arrives with a later version.
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
