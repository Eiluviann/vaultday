import Foundation
import Observation

/// The task the register holds: the last one yanked or cut.
public struct RegisterItem: Equatable, Sendable {
	public let taskId: String
	public let text: String
}

/// One undoable change as it was written: each task line before and after,
/// as the core reported them, and each order before and after. Undo puts
/// the old lines back exactly, field order included, and only while the
/// lines still read as the change left them. The core keeps no operation
/// log of its own, so the app keeps this one; every step writes through it.
public struct PlanChange: Equatable, Sendable {
	public enum Step: Equatable, Sendable {
		case line(taskId: String, before: String, after: String)
		case order(date: String, before: [String], after: [String])
	}

	/// What the change did, for the undo message: "Cut task-117 to the backlog".
	public let title: String
	public let steps: [Step]
}

/// The register (`yy`, `dd`, `p`, `P`), the changes to the selected task
/// (`x`, `>>`, `<<`, `⌥j`, `⌥k`, `cc` and the rest) and their undo and redo
/// (`u`, `^r`). Each change writes through the core via ``PlanEditing`` and
/// is kept for undo; a new change clears the redo stack.
@MainActor
@Observable
public final class PlanRegister {
	/// How many changes undo goes back.
	public static let historyLimit = 100

	public private(set) var item: RegisterItem?
	/// The last thing the register did or why it could not, for the toast.
	public private(set) var message: String?
	public private(set) var undoStack: [PlanChange] = []
	public private(set) var redoStack: [PlanChange] = []

	@ObservationIgnored private let plan: PlanEditing
	@ObservationIgnored private let copyToPasteboard: @MainActor (String) -> Void

	/// A step of a change before it is written.
	private enum Intent {
		case edit(taskId: String, TaskLineEdit)
		case order(date: String, after: [String])
	}

	private enum DayInput {
		/// A day, or `nil` for a cleared field.
		case day(String?)
		case invalid
	}

	public init(plan: PlanEditing, copyToPasteboard: @escaping @MainActor (String) -> Void) {
		self.plan = plan
		self.copyToPasteboard = copyToPasteboard
	}

	public var canUndo: Bool { !undoStack.isEmpty }
	public var canRedo: Bool { !redoStack.isEmpty }

	/// Forgets the history and the register: another workspace opened.
	public func reset() {
		item = nil
		message = nil
		undoStack = []
		redoStack = []
	}

	/// The task's line as the vault holds it now.
	public func lineState(of taskId: String) -> TaskLineState? {
		plan.lineState(of: taskId)
	}

	/// `yy`: the task into the register, its id onto the pasteboard.
	public func yank(_ taskId: String) {
		guard let state = plan.lineState(of: taskId) else { return fail("No task \(taskId)") }
		item = RegisterItem(taskId: taskId, text: state.text)
		copyToPasteboard(taskId)
		message = "Yanked \(taskId)"
	}

	/// `dd`: the task off its day into the backlog, and into the register.
	public func cut(_ taskId: String) {
		guard let state = plan.lineState(of: taskId) else { return fail("No task \(taskId)") }
		item = RegisterItem(taskId: taskId, text: state.text)
		let date = plan.shownDate
		let after = plan.listOrder(on: date).filter { $0 != taskId }
		commit("Cut \(taskId) to the backlog", [.edit(taskId: taskId, .schedule(nil)), .order(date: date, after: after)])
	}

	/// `p` and `P`: the register's task scheduled on the day shown, placed
	/// after or before `anchor` (the selection), or at the end without one.
	public func paste(after isAfter: Bool, anchor: String?) {
		guard let item else { return fail("The register is empty; yy or dd fills it") }
		let date = plan.shownDate
		let after = Self.placing(item.taskId, relativeTo: anchor, isAfter: isAfter, in: plan.listOrder(on: date))
		commit("Pasted \(item.taskId) on \(date)", [.edit(taskId: item.taskId, .schedule(date)), .order(date: date, after: after)])
	}

	/// `x`: done as of today.
	public func complete(_ taskId: String) {
		guard let state = plan.lineState(of: taskId), !state.isDone else { return }
		commit("Done \(taskId)", [.edit(taskId: taskId, .done(on: plan.todayDate))])
	}

	/// `>>`: scheduled on the day after the one shown.
	public func pushToTomorrow(_ taskId: String) {
		guard let next = Self.day(after: plan.shownDate) else { return }
		commit("Pushed \(taskId) to \(next)", [.edit(taskId: taskId, .schedule(next))])
	}

	/// `<<`: scheduled today.
	public func pullToToday(_ taskId: String) {
		commit("Pulled \(taskId) to today", [.edit(taskId: taskId, .schedule(plan.todayDate))])
	}

	/// `⌥j` and `⌥k`: one place down (`+1`) or up (`-1`) in the day's list.
	public func step(_ taskId: String, by step: Int) {
		let date = plan.shownDate
		var order = plan.listOrder(on: date)
		guard let from = order.firstIndex(of: taskId) else { return }
		let to = min(max(from + step, 0), order.count - 1)
		// Only unfinished tasks move; the done ones stay below them.
		guard to != from, plan.lineState(of: order[to])?.isDone != true else { return }
		order.remove(at: from)
		order.insert(taskId, at: to)
		commit("Moved \(taskId)", [.order(date: date, after: order)])
	}

	/// `cc`, `ce`, `cd`, `cs`: sets a field. An empty value clears an
	/// estimate or a day; an empty text is refused. A day is `today`,
	/// `tomorrow` or `YYYY-MM-DD`.
	public func change(_ field: TaskField, of taskId: String, to rawValue: String) {
		let value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
		let optional = value.isEmpty ? nil : value
		let edit: TaskLineEdit
		switch field {
		case .text:
			guard !value.isEmpty else { return fail("A task needs text") }
			edit = .text(value)
		case .estimate:
			edit = .estimate(optional)
		case .due:
			guard case let .day(day) = resolveDay(optional) else { return fail("Not a day: \(value)") }
			edit = .due(day)
		case .scheduled:
			guard case let .day(day) = resolveDay(optional) else { return fail("Not a day: \(value)") }
			edit = .schedule(day)
		}
		commit("Changed the \(field.rawValue) of \(taskId)", [.edit(taskId: taskId, edit)])
	}

	/// `u`: puts the last change back.
	public func undo() {
		guard let change = undoStack.popLast() else { return fail("Nothing to undo") }
		if let error = replay(change, isForward: false) {
			return fail(error)
		}
		redoStack.append(change)
		message = "Undid: \(change.title)"
		plan.refresh()
	}

	/// `^r`: makes the last undone change again.
	public func redo() {
		guard let change = redoStack.popLast() else { return fail("Nothing to redo") }
		if let error = replay(change, isForward: true) {
			return fail(error)
		}
		undoStack.append(change)
		message = "Redid: \(change.title)"
		plan.refresh()
	}

	// MARK: Private

	/// Writes the intents in order and keeps the change for undo. On a
	/// refusal the steps already written are put back and nothing is kept.
	private func commit(_ title: String, _ intents: [Intent]) {
		var steps: [PlanChange.Step] = []
		for intent in intents {
			switch write(intent) {
			case let .success(step):
				steps.append(step)
			case let .failure(refusal):
				_ = run(Array(steps.reversed()), isForward: false)
				fail(refusal.message)
				plan.refresh()
				return
			}
		}
		undoStack.append(PlanChange(title: title, steps: steps))
		if undoStack.count > Self.historyLimit { undoStack.removeFirst(undoStack.count - Self.historyLimit) }
		redoStack = []
		message = title
		plan.refresh()
	}

	private struct Refusal: Error {
		let message: String
	}

	private func write(_ intent: Intent) -> Result<PlanChange.Step, Refusal> {
		switch intent {
		case let .edit(taskId, edit):
			switch plan.apply(edit, to: taskId) {
			case let .written(old, new): return .success(.line(taskId: taskId, before: old, after: new))
			case let .refused(message): return .failure(Refusal(message: message))
			}
		case let .order(date, after):
			let before = plan.listOrder(on: date)
			if let message = plan.storeOrder(after, on: date) { return .failure(Refusal(message: message)) }
			return .success(.order(date: date, before: before, after: after))
		}
	}

	/// Undo runs the steps backward in reverse order, redo forward in order;
	/// a refusal puts back the steps it already ran. The change stays
	/// where it was on the stack it came from when refused.
	private func replay(_ change: PlanChange, isForward: Bool) -> String? {
		let steps = isForward ? change.steps : Array(change.steps.reversed())
		if let error = run(steps, isForward: isForward) {
			if isForward { redoStack.append(change) } else { undoStack.append(change) }
			plan.refresh()
			return error
		}
		return nil
	}

	private func run(_ steps: [PlanChange.Step], isForward: Bool) -> String? {
		var done: [PlanChange.Step] = []
		for step in steps {
			if let error = apply(step, isForward: isForward) {
				for undone in done.reversed() { _ = apply(undone, isForward: !isForward) }
				return error
			}
			done.append(step)
		}
		return nil
	}

	private func apply(_ step: PlanChange.Step, isForward: Bool) -> String? {
		switch step {
		case let .line(taskId, before, after):
			isForward ? plan.restore(taskId, line: after, expecting: before) : plan.restore(taskId, line: before, expecting: after)
		case let .order(date, before, after):
			plan.storeOrder(isForward ? after : before, on: date)
		}
	}

	private func fail(_ text: String) {
		message = text
	}

	private func resolveDay(_ text: String?) -> DayInput {
		guard let text else { return .day(nil) }
		switch text.lowercased() {
		case "today": return .day(plan.todayDate)
		case "tomorrow": return Self.day(after: plan.todayDate).map { .day($0) } ?? .invalid
		default: return DayText.date(from: text) == nil ? .invalid : .day(text)
		}
	}

	static func placing(_ taskId: String, relativeTo anchor: String?, isAfter: Bool, in order: [String]) -> [String] {
		var result = order.filter { $0 != taskId }
		guard let anchor, let index = result.firstIndex(of: anchor) else {
			result.append(taskId)
			return result
		}
		result.insert(taskId, at: isAfter ? index + 1 : index)
		return result
	}

	static func day(after date: String) -> String? {
		guard let day = DayText.date(from: date), let next = Calendar.current.date(byAdding: .day, value: 1, to: day) else { return nil }
		return DayText.string(next)
	}
}
