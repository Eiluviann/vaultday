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
///
/// Entering a container (a pane jump, a step across panes, a side panel tab,
/// a workspace) follows the scheme: under Vim motions the focus sits on the
/// container itself, with no item selected until `j` or `k`; under Standard
/// it lands on the container's first item.
@MainActor
@Observable
public final class PaneFocusModel {
	public private(set) var focusedPane: Pane = .today
	public private(set) var isSidebarCollapsed = false
	/// The item last opened with ⏎, by pane and index.
	public private(set) var opened: OpenedItem?
	public private(set) var scheme: KeyScheme
	/// Whether the pointer entering an item selects it (Settings, Keyboard).
	public var hoverSelects = true
	/// The panes on screen, in the order `h` and `l` step through: the Home
	/// window's four, or the story screen's two.
	public private(set) var paneOrder: [Pane] = Pane.homeOrder

	private var selections: [Pane: Int] = [:]
	private var itemCounts: [Pane: Int] = [:]
	private var visibleRows: [Pane: Int] = [:]
	/// Panes whose focus sits on the container, not on an item.
	private var containerFocused: Set<Pane> = []

	public init(focusedPane: Pane = .today, isSidebarCollapsed: Bool = false, scheme: KeyScheme = .standard) {
		self.focusedPane = focusedPane
		self.isSidebarCollapsed = isSidebarCollapsed
		self.scheme = scheme
	}

	/// The selected index in `pane`, `nil` when it holds no items or the
	/// focus sits on the pane itself.
	public func selection(in pane: Pane) -> Int? {
		guard let count = itemCounts[pane], count > 0, !containerFocused.contains(pane) else { return nil }
		return min(selections[pane] ?? 0, count - 1)
	}

	/// Whether the focus sits on `pane` itself rather than on one of its items.
	public func isOnContainer(_ pane: Pane) -> Bool {
		containerFocused.contains(pane)
	}

	/// Tells the model how many items `pane` lists now.
	public func setItemCount(_ count: Int, in pane: Pane) {
		itemCounts[pane] = max(0, count)
	}

	/// Tells the model how many rows of `pane` fit on screen, for `^d` and `^u`.
	public func setVisibleRowCount(_ count: Int, in pane: Pane) {
		visibleRows[pane] = max(1, count)
	}

	/// The screen changed: `order` is its panes. Focus on a pane that left
	/// the screen moves to the first pane that stays.
	public func setPaneOrder(_ order: [Pane]) {
		guard !order.isEmpty else { return }
		paneOrder = order
		if !order.contains(focusedPane) {
			focus(order.contains(.today) ? .today : order[0])
		}
	}

	/// Switches the scheme the focus rules follow.
	public func setScheme(_ scheme: KeyScheme) {
		self.scheme = scheme
		if scheme == .standard {
			containerFocused = []
		}
	}

	/// Selects an item with the pointer, focusing its pane.
	public func select(_ index: Int, in pane: Pane) {
		focusedPane = pane
		containerFocused.remove(pane)
		selections[pane] = clamp(index, in: pane)
	}

	/// The pointer entered an item: it selects it when hover selects is on.
	/// `true` when the selection moved.
	@discardableResult
	public func hover(_ index: Int, in pane: Pane) -> Bool {
		guard hoverSelects else { return false }
		select(index, in: pane)
		return true
	}

	/// Enters a container: focuses `pane` by the scheme's rule.
	public func focus(_ pane: Pane) {
		focusedPane = pane
		switch scheme {
		case .vim:
			containerFocused.insert(pane)
		case .standard:
			containerFocused.remove(pane)
			selections[pane] = 0
		}
	}

	/// Applies a dispatched action.
	public func apply(_ action: KeyAction) {
		switch action {
		case let .focusPane(pane):
			focus(pane)
		case .moveDown, .nextMatch:
			moveSelection(by: 1)
		case .moveUp, .previousMatch:
			moveSelection(by: -1)
		case .moveHalfPageDown:
			moveSelection(by: halfPage)
		case .moveHalfPageUp:
			moveSelection(by: -halfPage)
		case .moveLeft:
			movePane(by: -1)
		case .moveRight:
			movePane(by: 1)
		case .moveFirst:
			containerFocused.remove(focusedPane)
			selections[focusedPane] = 0
		case .moveLast:
			containerFocused.remove(focusedPane)
			selections[focusedPane] = max(0, (itemCounts[focusedPane] ?? 0) - 1)
		case .open:
			opened = selection(in: focusedPane).map { OpenedItem(pane: focusedPane, index: $0) }
		case .toggleSidebar:
			isSidebarCollapsed.toggle()
		case .addTask, .addTaskAfter, .addTaskBefore, .completeTask, .moveTaskUp, .moveTaskDown, .togglePin, .toggleTimer,
		     .showBrowse, .focusBrowseQuery:
			focusedPane = .today
		case .showWorkspace, .showAllWorkspaces, .nextWorkspace, .previousWorkspace:
			focus(.today)
		case .showDayLog, .showSidePanelTab, .nextSidePanelTab, .previousSidePanelTab, .appendDayLog:
			focus(.sidePanel)
		case .showStoryPanelTab, .nextStoryPanelTab, .previousStoryPanelTab:
			focus(.storyPanel)
		default:
			break
		}
	}

	private var halfPage: Int {
		max(1, (visibleRows[focusedPane] ?? 10) / 2)
	}

	private func moveSelection(by step: Int) {
		guard let count = itemCounts[focusedPane], count > 0 else { return }
		// From the container, `j` lands on the first item and `k` on the last.
		if containerFocused.remove(focusedPane) != nil {
			selections[focusedPane] = step > 0 ? 0 : count - 1
			return
		}
		let current = selection(in: focusedPane) ?? 0
		selections[focusedPane] = clamp(current + step, in: focusedPane)
	}

	private func movePane(by step: Int) {
		let order = paneOrder
		guard let index = order.firstIndex(of: focusedPane) else { return }
		let target = order[min(max(index + step, 0), order.count - 1)]
		guard target != focusedPane else { return }
		focus(target)
	}

	private func clamp(_ index: Int, in pane: Pane) -> Int {
		min(max(index, 0), max(0, (itemCounts[pane] ?? 0) - 1))
	}
}
