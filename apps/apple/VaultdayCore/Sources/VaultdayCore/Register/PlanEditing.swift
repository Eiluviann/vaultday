import Foundation

/// A task line's fields as the vault holds them, enough to put a change back.
public struct TaskLineState: Equatable, Sendable {
	public var text: String
	public var scheduled: String?
	public var due: String?
	public var estimate: String?
	/// The completion day; `nil` while the task is open.
	public var completion: String?

	public init(text: String, scheduled: String? = nil, due: String? = nil, estimate: String? = nil, completion: String? = nil) {
		self.text = text
		self.scheduled = scheduled
		self.due = due
		self.estimate = estimate
		self.completion = completion
	}

	public var isDone: Bool { completion != nil }
}

/// One change to a task line, as the core applies it. Days are `YYYY-MM-DD`;
/// `nil` removes the field.
public enum TaskLineEdit: Equatable, Sendable {
	case done(on: String)
	case reopen
	case schedule(String?)
	case due(String?)
	case estimate(String?)
	case text(String)
}

/// What a line write did: the line before and after, as the core reported
/// them, or why it refused.
public enum LineWrite: Equatable, Sendable {
	case written(old: String, new: String)
	case refused(String)
}

/// The field a change key (`cc`, `ce`, `cd`, `cs`) edits.
public enum TaskField: String, CaseIterable, Sendable {
	case text
	case estimate
	case due
	case scheduled

	/// The change key's action.
	public var action: KeyAction {
		switch self {
		case .text: .changeText
		case .estimate: .changeEstimate
		case .due: .changeDue
		case .scheduled: .changeScheduled
		}
	}

	public init?(_ action: KeyAction) {
		guard let field = Self.allCases.first(where: { $0.action == action }) else { return nil }
		self = field
	}

	/// The field's current value in `state`, as the editor starts with it.
	public func value(in state: TaskLineState) -> String {
		switch self {
		case .text: state.text
		case .estimate: state.estimate ?? ""
		case .due: state.due ?? ""
		case .scheduled: state.scheduled ?? ""
		}
	}
}

/// What the register and its undo need from the day's plan: read a task
/// line, change it through the core, and read and store a day's order.
/// ``DayPlanModel`` is the app's; the tests use a fake.
@MainActor
public protocol PlanEditing: AnyObject {
	/// The day shown, `YYYY-MM-DD`.
	var shownDate: String { get }
	/// The host's day, `YYYY-MM-DD`.
	var todayDate: String { get }
	/// The task's line as the vault holds it now; `nil` when there is no such task.
	func lineState(of taskId: String) -> TaskLineState?
	/// Applies one edit through the core.
	func apply(_ edit: TaskLineEdit, to taskId: String) -> LineWrite
	/// Puts the task's line back to `line` while it still reads `current`;
	/// the refusal's message when it does not.
	func restore(_ taskId: String, line: String, expecting current: String) -> String?
	/// The list order of `date`, task ids.
	func listOrder(on date: String) -> [String]
	/// Stores the list order of `date`; the refusal's message when it fails.
	func storeOrder(_ taskIds: [String], on date: String) -> String?
	/// Reads the plan again after a change.
	func refresh()
}
