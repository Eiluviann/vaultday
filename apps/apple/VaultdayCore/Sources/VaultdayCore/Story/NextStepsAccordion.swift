import Foundation

/// The Next Steps accordion: the note's steps in its order, done steps in
/// place, one line each. Only the selected or the hovered step opens to
/// show its controls; the hovered one wins while the pointer is on it. The
/// list opens scrolled to the first open step, with a caption counting the
/// completed steps above it.
public struct NextStepsAccordion: Equatable, Sendable {
	public let steps: [StoryStep]
	public private(set) var selectedId: String?
	public private(set) var hoveredId: String?

	public init(steps: [StoryStep]) {
		self.steps = steps
	}

	/// The step that shows its controls now.
	public var expandedId: String? {
		hoveredId ?? selectedId
	}

	public func showsControls(_ step: StoryStep) -> Bool {
		step.id == expandedId
	}

	/// Where the list opens: the first step not done, else the last step.
	public var initialScrollId: String? {
		(steps.first { !$0.isDone } ?? steps.last)?.id
	}

	/// The selected step's place, for the pane's selection.
	public var selectedIndex: Int? {
		selectedId.flatMap { id in steps.firstIndex { $0.id == id } }
	}

	/// Done steps before the first open one, which the list scrolls past.
	public var completedAbove: Int {
		let firstOpen = steps.firstIndex { !$0.isDone } ?? steps.count
		return steps[..<firstOpen].count(where: \.isDone)
	}

	/// "2 completed steps above", "1 completed step above"; `nil` when none.
	public var completedCaption: String? {
		switch completedAbove {
		case 0: nil
		case 1: "1 completed step above"
		case let count: "\(count) completed steps above"
		}
	}

	/// Selects the step at `index`; `nil` or a place outside the list clears it.
	public mutating func select(index: Int?) {
		guard let index, steps.indices.contains(index) else {
			selectedId = nil
			return
		}
		selectedId = steps[index].id
	}

	public mutating func select(id: String?) {
		selectedId = id.flatMap { id in steps.contains { $0.id == id } ? id : nil }
	}

	/// The pointer entered a step, or left them all (`nil`).
	public mutating func hover(id: String?) {
		hoveredId = id.flatMap { id in steps.contains { $0.id == id } ? id : nil }
	}
}
