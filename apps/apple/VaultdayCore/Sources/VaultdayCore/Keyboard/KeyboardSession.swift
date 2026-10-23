import Foundation
import Observation

/// A change key waiting for its value: `cc` and the rest open an editor on
/// the selected task's field.
public struct TaskChangeRequest: Equatable, Sendable {
	public let field: TaskField
	public let taskId: String
	/// The field's value now, as the editor starts with it.
	public let initialValue: String
	/// The editor's title, from the key table.
	public let title: String
}

/// The keyboard's path from a stroke to its effect, shared by the app and
/// the tests: the dispatcher resolves the stroke, the focus model moves the
/// selection, the register runs the changes it owns, and everything else
/// goes to ``onAction`` (the app's own handlers). Controls and the palette
/// that name an action go through ``run(_:)`` the same way.
@MainActor
@Observable
public final class KeyboardSession {
	/// Changes `.` repeats on the selection.
	public static let repeatableActions: Set<KeyAction> = [
		.completeTask, .cut, .pasteAfter, .pasteBefore, .pushToTomorrow, .pullToToday,
		.moveTaskUp, .moveTaskDown, .togglePin,
	]
	/// The repeatable changes that act on the selected task: without one,
	/// they do nothing, so `.` records nothing (paste has its own anchor rule).
	static let selectionChanges: Set<KeyAction> = repeatableActions.subtracting([.pasteAfter, .pasteBefore])

	public let dispatcher: KeyDispatcher
	public let focus: PaneFocusModel
	public let register: PlanRegister
	/// The last change, for `.`.
	public private(set) var lastChange: KeyAction?
	/// The editor a change key opened, until it is saved or cancelled.
	public private(set) var changeRequest: TaskChangeRequest?
	/// Whether the selected item's more-actions menu is open by `\`.
	public private(set) var isItemMenuOpen = false

	/// The task the selection is on, when it is one.
	@ObservationIgnored public var selectedTaskId: () -> String? = { nil }
	/// The signal the Inbox selection is on, when it is one.
	@ObservationIgnored public var selectedSignalId: () -> String? = { nil }
	/// The app's handler of every action the session does not run itself.
	@ObservationIgnored public var onAction: (KeyAction) -> Void = { _ in }
	/// Called after the session ran one of its own actions, so the app reads the plan again.
	@ObservationIgnored public var onOwnAction: (KeyAction) -> Void = { _ in }
	/// Whether an action applies where the app is now (Browse keys outside Browse do not).
	@ObservationIgnored public var isEnabled: (KeyAction) -> Bool = { _ in true }
	/// Esc's first job: closes the app's top transient layer (the palette, a
	/// draft, a preview, a toast); `true` when one closed. When none did,
	/// Esc moves the focus one level up.
	@ObservationIgnored public var dismissTransient: () -> Bool = { false }

	public init(dispatcher: KeyDispatcher, focus: PaneFocusModel, register: PlanRegister) {
		self.dispatcher = dispatcher
		self.focus = focus
		self.register = register
	}

	/// The item menu shows while `\` opened it or a pending stroke completes inside it.
	public var showsItemMenu: Bool {
		isItemMenuOpen || dispatcher.opensItemMenu
	}

	/// Whether the item menu shows on the selected item of `pane`: the Inbox
	/// has its signal menu, every other pane the task menu.
	public func showsItemMenu(in pane: Pane) -> Bool {
		showsItemMenu && focus.focusedPane == pane
	}

	/// Feeds one stroke; an action it completes runs.
	@discardableResult
	public func handle(_ stroke: KeyStroke) -> KeyResolution {
		let resolution = dispatcher.handle(stroke, in: focus.focusedPane, screen: focus.paneOrder)
		if case let .action(action) = resolution {
			run(action)
		}
		return resolution
	}

	/// Switches both the dispatcher and the focus rules to `scheme`.
	public func setScheme(_ scheme: KeyScheme) {
		dispatcher.setScheme(scheme)
		focus.setScheme(scheme)
	}

	/// Runs one action, from a key, a control or the palette.
	public func run(_ action: KeyAction) {
		guard isEnabled(action) else { return }
		// Esc closes the session's own menu or editor first, then the app's
		// transient layer, and only then moves one level up the focus tree.
		if action == .dismiss {
			if isItemMenuOpen || changeRequest != nil {
				isItemMenuOpen = false
				changeRequest = nil
			} else if !dismissTransient() {
				focus.ascend()
			}
			return
		}
		// Written only on a change, so a plain move invalidates no menu view.
		if action != .openItemMenu, isItemMenuOpen { isItemMenuOpen = false }
		focus.apply(action)
		if action == .repeatLast {
			repeatLastChange()
			return
		}
		let isRecordable = Self.repeatableActions.contains(action)
			&& (!Self.selectionChanges.contains(action) || selectedTaskId() != nil)
		if runOwnAction(action) {
			if isRecordable { lastChange = action }
			onOwnAction(action)
			return
		}
		if isRecordable { lastChange = action }
		onAction(action)
	}

	/// Saves the change editor's value through the register.
	public func submitChange(_ value: String) {
		guard let request = changeRequest else { return }
		changeRequest = nil
		register.change(request.field, of: request.taskId, to: value)
		onOwnAction(request.field.action)
	}

	public func cancelChange() {
		changeRequest = nil
	}

	/// Closes the item menu `\` opened.
	public func closeItemMenu() {
		isItemMenuOpen = false
	}

	/// Another workspace opened: its tasks are not the register's.
	public func reset() {
		register.reset()
		lastChange = nil
		changeRequest = nil
		isItemMenuOpen = false
		dispatcher.cancelPending()
	}

	/// `true` when the action is the session's to run.
	private func runOwnAction(_ action: KeyAction) -> Bool {
		let taskId = selectedTaskId()
		switch action {
		case .yank:
			if let taskId { register.yank(taskId) }
		case .cut:
			if let taskId { register.cut(taskId) }
		case .pasteAfter:
			register.paste(after: true, anchor: taskId)
		case .pasteBefore:
			register.paste(after: false, anchor: taskId)
		case .completeTask:
			if let taskId { register.complete(taskId) }
		case .pushToTomorrow:
			if let taskId { register.pushToTomorrow(taskId) }
		case .pullToToday:
			if let taskId { register.pullToToday(taskId) }
		case .moveTaskUp:
			if let taskId { register.step(taskId, by: -1) }
		case .moveTaskDown:
			if let taskId { register.step(taskId, by: 1) }
		case .undo:
			register.undo()
		case .redo:
			register.redo()
		case .changeText, .changeEstimate, .changeDue, .changeScheduled:
			openChange(action, taskId: taskId)
		case .openItemMenu:
			let selection = focus.focusedPane == .inbox ? selectedSignalId() : taskId
			isItemMenuOpen = selection != nil && !isItemMenuOpen
		default:
			return false
		}
		return true
	}

	private func openChange(_ action: KeyAction, taskId: String?) {
		guard let taskId, let field = TaskField(action), let state = register.lineState(of: taskId) else { return }
		changeRequest = TaskChangeRequest(
			field: field,
			taskId: taskId,
			initialValue: field.value(in: state),
			title: dispatcher.title(for: action) ?? field.rawValue,
		)
	}

	private func repeatLastChange() {
		guard let lastChange else { return }
		if Self.selectionChanges.contains(lastChange), selectedTaskId() == nil { return }
		focus.apply(lastChange)
		if runOwnAction(lastChange) {
			onOwnAction(lastChange)
		} else {
			onAction(lastChange)
		}
	}
}
