import Foundation
import Observation

/// An item opened with ⏎.
public struct OpenedItem: Equatable, Sendable {
	public let pane: Pane
	public let index: Int
}

/// Which pane has focus and which item is selected in each pane. The
/// selection is the only active state: it moves with the keys or the
/// pointer and stays where it is when the pointer leaves. Panes report how
/// many items they hold; the selection is kept inside that range.
@MainActor
@Observable
public final class PaneFocusModel {
	public private(set) var focusedPane: Pane = .today
	public private(set) var isSidebarCollapsed = false
	/// The item last opened with ⏎, by pane and index.
	public private(set) var opened: OpenedItem?

	private var selections: [Pane: Int] = [:]
	private var itemCounts: [Pane: Int] = [:]

	public init(focusedPane: Pane = .today, isSidebarCollapsed: Bool = false) {
		self.focusedPane = focusedPane
		self.isSidebarCollapsed = isSidebarCollapsed
	}

	/// The selected index in `pane`, `nil` when it holds no items.
	public func selection(in pane: Pane) -> Int? {
		guard let count = itemCounts[pane], count > 0 else { return nil }
		return min(selections[pane] ?? 0, count - 1)
	}

	/// Tells the model how many items `pane` lists now.
	public func setItemCount(_ count: Int, in pane: Pane) {
		itemCounts[pane] = max(0, count)
	}

	/// Selects an item with the pointer, focusing its pane.
	public func select(_ index: Int, in pane: Pane) {
		focusedPane = pane
		selections[pane] = clamp(index, in: pane)
	}

	/// Focuses a pane.
	public func focus(_ pane: Pane) {
		focusedPane = pane
	}

	/// Applies a dispatched action.
	public func apply(_ action: KeyAction) {
		switch action {
		case let .focusPane(pane):
			focusedPane = pane
		case .moveDown:
			moveSelection(by: 1)
		case .moveUp:
			moveSelection(by: -1)
		case .moveLeft:
			movePane(by: -1)
		case .moveRight:
			movePane(by: 1)
		case .moveFirst:
			selections[focusedPane] = 0
		case .moveLast:
			selections[focusedPane] = max(0, (itemCounts[focusedPane] ?? 0) - 1)
		case .open:
			opened = selection(in: focusedPane).map { OpenedItem(pane: focusedPane, index: $0) }
		case .toggleSidebar:
			isSidebarCollapsed.toggle()
		case .addTask, .completeTask, .moveTaskUp, .moveTaskDown, .togglePin, .toggleTimer:
			focusedPane = .today
		case .showAllWorkspaces, .showWorkspace, .focusSearch, .replan, .autoSchedule,
		     .previousDay, .nextDay, .showToday, .dismiss:
			break
		}
	}

	private func moveSelection(by step: Int) {
		guard let current = selection(in: focusedPane) else { return }
		selections[focusedPane] = clamp(current + step, in: focusedPane)
	}

	private func movePane(by step: Int) {
		let order = Pane.allCases
		guard let index = order.firstIndex(of: focusedPane) else { return }
		let target = min(max(index + step, 0), order.count - 1)
		focusedPane = order[target]
	}

	private func clamp(_ index: Int, in pane: Pane) -> Int {
		min(max(index, 0), max(0, (itemCounts[pane] ?? 0) - 1))
	}
}
