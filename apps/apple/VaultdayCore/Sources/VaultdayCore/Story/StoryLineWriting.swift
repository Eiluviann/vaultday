import Foundation

/// The only changes the story screen makes to a note: a task line's
/// checkbox, its scheduled day and its estimate. There is deliberately no
/// case for a line's text, a heading or a paragraph: free text is edited in
/// Obsidian, never here.
public enum StoryLineEdit: Equatable, Sendable {
	/// `[x]` with `[completion:: day]`.
	case tick(on: String)
	/// `[ ]` without a completion day.
	case reopen
	/// Sets or removes `[scheduled:: day]`.
	case schedule(String?)
	/// Sets or removes `[estimate:: …]`.
	case estimate(String?)
}

/// Where the story screen's line edits go. The app's is the core writer,
/// which changes the one line carrying the task's `[id:: ]`; tests use a fake.
@MainActor
public protocol StoryLineWriting: AnyObject {
	func apply(_ edit: StoryLineEdit, to taskId: String) -> LineWrite
}

/// The core writer behind the story screen.
@MainActor
final class CoreStoryLines: StoryLineWriting {
	private let vault: WorkspaceHandle

	init(vault: WorkspaceHandle) {
		self.vault = vault
	}

	func apply(_ edit: StoryLineEdit, to taskId: String) -> LineWrite {
		do {
			let change = try editTask(workspace: vault, id: taskId, edit: Self.input(for: edit))
			return .written(old: change.oldLine ?? change.newLine, new: change.newLine)
		} catch {
			return .refused(coreErrorMessage(error))
		}
	}

	/// The core edit for a story screen edit; never a text edit.
	static func input(for edit: StoryLineEdit) -> TaskEditInput {
		switch edit {
		case let .tick(day): .done(date: day)
		case .reopen: .reopen
		case let .schedule(day): .schedule(date: day)
		case let .estimate(estimate): .estimate(estimate: estimate)
		}
	}
}
