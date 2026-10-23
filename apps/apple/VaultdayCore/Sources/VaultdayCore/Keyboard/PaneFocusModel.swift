import Foundation
import Observation

/// An item opened with ⏎.
public struct OpenedItem: Equatable, Sendable {
	public let pane: Pane
	public let index: Int
}

/// A strip of controls inside a pane, above its list: the Today header.
public enum FocusSection: Hashable, Sendable {
	/// The Today header: the today chip, ‹ ›, the span switch, `=` and the wand.
	case dayHeader

	/// The pane the section sits in.
	public var pane: Pane {
		switch self {
		case .dayHeader: .today
		}
	}
}

/// A text input holding the keys, by the name the app gives it, and the
/// pane Esc returns to.
public struct FocusedInput: Equatable, Sendable {
	public let name: String
	public let pane: Pane

	public init(name: String, pane: Pane) {
		self.name = name
		self.pane = pane
	}
}

/// Where the keys go, the deepest level that holds focus: the window's
/// root is implied, then a pane, a section in it, a row, or an input.
public enum FocusLevel: Equatable, Sendable {
	/// The pane itself, no row selected (Vim motions only).
	case pane(Pane)
	/// A control of a section, by its place.
	case section(FocusSection, control: Int)
	/// A row of a pane's list.
	case row(Pane, index: Int)
	case input(FocusedInput)
}

/// One pane's selection, kept apart from the other panes' so a move in one
/// pane invalidates only the views that read that pane.
@MainActor
@Observable
final class PaneSlot {
	var selection = 0
	var itemCount = 0
	var visibleRows = 10
	/// The focus sits on the pane itself, not on an item.
	var isOnContainer = false
}

/// Which pane has focus and which item is selected in each pane: the focus
/// tree's window (implied), pane, section, row and input levels. The
/// selection is the only active state: it moves with the keys or the
/// pointer and stays where it is when the pointer leaves. Panes report how
/// many items they hold; the selection is kept inside that range.
///
/// Entering a container (a pane jump, a step across panes, a side panel tab,
/// a workspace) follows the scheme: under Vim motions the focus sits on the
/// container itself, with no item selected until `j` or `k`; under Standard
/// it lands on the container's first item. The sidebar lands on its current
/// row under both. Esc (``ascend()``) moves one level up.
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
	/// The section holding the keys, if any, and the control it is on.
	public private(set) var section: FocusSection?
	public private(set) var sectionControl = 0
	/// The text input holding the keys, if any.
	public private(set) var input: FocusedInput?
	/// Panes that land on their current row rather than the container under
	/// Vim motions: the sidebar, so `g1` puts the highlight where it was.
	public let panesLandingOnRow: Set<Pane> = [.sidebar]

	@ObservationIgnored private let slots: [Pane: PaneSlot] = Dictionary(uniqueKeysWithValues: Pane.allCases.map { ($0, PaneSlot()) })
	/// How many controls each section shows now.
	@ObservationIgnored private var sectionControlCounts: [FocusSection: Int] = [:]
	/// The section above each pane's list.
	private static let sectionAbove: [Pane: FocusSection] = [.today: .dayHeader]

	public init(focusedPane: Pane = .today, isSidebarCollapsed: Bool = false, scheme: KeyScheme = .standard) {
		self.focusedPane = focusedPane
		self.isSidebarCollapsed = isSidebarCollapsed
		self.scheme = scheme
	}

	/// The deepest level that holds the keys.
	public var level: FocusLevel {
		if let input { return .input(input) }
		if let section { return .section(section, control: sectionControl) }
		if let index = selection(in: focusedPane) { return .row(focusedPane, index: index) }
		return .pane(focusedPane)
	}

	/// The selected index in `pane`, `nil` when it holds no items, the focus
	/// sits on the pane itself, or a section of the pane holds the keys.
	public func selection(in pane: Pane) -> Int? {
		let slot = slot(pane)
		guard slot.itemCount > 0, !slot.isOnContainer, section?.pane != pane else { return nil }
		return min(slot.selection, slot.itemCount - 1)
	}

	/// Whether the focus sits on `pane` itself rather than on one of its items.
	public func isOnContainer(_ pane: Pane) -> Bool {
		slot(pane).isOnContainer
	}

	/// The control of `section` the keys are on, `nil` while the section does not hold them.
	public func control(in section: FocusSection) -> Int? {
		self.section == section ? sectionControl : nil
	}

	/// Tells the model how many items `pane` lists now.
	public func setItemCount(_ count: Int, in pane: Pane) {
		let slot = slot(pane)
		let count = max(0, count)
		if slot.itemCount != count { slot.itemCount = count }
	}

	/// Tells the model how many rows of `pane` fit on screen, for `^d` and `^u`.
	public func setVisibleRowCount(_ count: Int, in pane: Pane) {
		let slot = slot(pane)
		let count = max(1, count)
		if slot.visibleRows != count { slot.visibleRows = count }
	}

	/// Tells the model how many controls `section` shows now.
	public func setControlCount(_ count: Int, in section: FocusSection) {
		sectionControlCounts[section] = max(0, count)
		if self.section == section {
			if count <= 0 { leaveSection() } else { sectionControl = min(sectionControl, count - 1) }
		}
	}

	/// Puts `pane`'s current row at `index` without focusing the pane: the
	/// sidebar's row follows the open workspace.
	public func setCurrentRow(_ index: Int, in pane: Pane) {
		let slot = slot(pane)
		let index = clamp(index, in: pane)
		if slot.selection != index { slot.selection = index }
	}

	/// The screen changed: `order` is its panes. Focus on a pane that left
	/// the screen moves to the first pane that stays.
	public func setPaneOrder(_ order: [Pane]) {
		guard !order.isEmpty else { return }
		if paneOrder != order { paneOrder = order }
		if !order.contains(focusedPane) {
			focus(order.contains(.today) ? .today : order[0])
		}
	}

	/// Switches the scheme the focus rules follow.
	public func setScheme(_ scheme: KeyScheme) {
		self.scheme = scheme
		if scheme == .standard {
			for slot in slots.values where slot.isOnContainer {
				slot.isOnContainer = false
			}
		}
	}

	/// Selects an item with the pointer, focusing its pane.
	public func select(_ index: Int, in pane: Pane) {
		setFocusedPane(pane)
		leaveSection()
		let slot = slot(pane)
		if slot.isOnContainer { slot.isOnContainer = false }
		let index = clamp(index, in: pane)
		if slot.selection != index { slot.selection = index }
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
		setFocusedPane(pane)
		leaveSection()
		let slot = slot(pane)
		if panesLandingOnRow.contains(pane) {
			if slot.isOnContainer { slot.isOnContainer = false }
			return
		}
		switch scheme {
		case .vim:
			if !slot.isOnContainer { slot.isOnContainer = true }
		case .standard:
			if slot.isOnContainer { slot.isOnContainer = false }
			if slot.selection != 0 { slot.selection = 0 }
		}
	}

	/// A text input took the keys; Esc in it comes back to `input.pane`.
	public func enterInput(_ input: FocusedInput) {
		setFocusedPane(input.pane)
		leaveSection()
		if self.input != input { self.input = input }
	}

	/// The input named `name` gave the keys up (Esc, a click elsewhere); the
	/// pane it belongs to holds them again, at the level it had before.
	public func leaveInput(named name: String) {
		guard input?.name == name else { return }
		input = nil
	}

	/// Esc: one level up the focus tree. Input to its pane, section to its
	/// pane (Vim: the container; Standard: the first row), row to the
	/// container under Vim motions. A pane, and a Standard row, stay put.
	/// `true` when the focus moved.
	@discardableResult
	public func ascend() -> Bool {
		if let input {
			leaveInput(named: input.name)
			return true
		}
		if let section {
			leaveSection()
			landBelowSection(in: section.pane, isFromEsc: true)
			return true
		}
		guard scheme == .vim, selection(in: focusedPane) != nil else { return false }
		slot(focusedPane).isOnContainer = true
		return true
	}

	/// Applies a dispatched action.
	public func apply(_ action: KeyAction) {
		if let section, applyInSection(section, action) { return }
		switch action {
		case let .focusPane(pane):
			focus(pane)
		case .moveDown:
			moveSelection(by: 1)
		case .moveUp:
			moveSelection(by: -1)
		case .moveHalfPageDown:
			moveSelection(by: halfPage, entersSection: false)
		case .moveHalfPageUp:
			moveSelection(by: -halfPage, entersSection: false)
		case .moveLeft:
			movePane(by: -1)
		case .moveRight:
			movePane(by: 1)
		case .moveFirst:
			select(0, in: focusedPane)
		case .moveLast:
			select(max(0, slot(focusedPane).itemCount - 1), in: focusedPane)
		case .open:
			let opened = selection(in: focusedPane).map { OpenedItem(pane: focusedPane, index: $0) }
			if self.opened != opened { self.opened = opened }
		case .toggleSidebar:
			isSidebarCollapsed.toggle()
		case .addTask, .addTaskAfter, .addTaskBefore, .completeTask, .moveTaskUp, .moveTaskDown, .togglePin, .toggleTimer,
		     .showBrowse, .focusBrowseQuery:
			setFocusedPane(.today)
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

	/// The section's own moves: `h` `l` across its controls, `j` down into
	/// the list below; `true` when the section took the action.
	private func applyInSection(_ section: FocusSection, _ action: KeyAction) -> Bool {
		let count = sectionControlCounts[section] ?? 0
		switch action {
		case .moveLeft:
			sectionControl = max(0, sectionControl - 1)
		case .moveRight:
			sectionControl = min(max(0, count - 1), sectionControl + 1)
		case .moveUp:
			break
		case .moveDown, .moveHalfPageDown:
			leaveSection()
			landBelowSection(in: section.pane, isFromEsc: false)
		case .moveFirst, .moveLast, .focusPane:
			leaveSection()
			return false
		case .open:
			// ⏎ activates the control; the app runs it.
			break
		default:
			return false
		}
		return true
	}

	private var halfPage: Int {
		max(1, slot(focusedPane).visibleRows / 2)
	}

	private func moveSelection(by step: Int, entersSection: Bool = true) {
		let slot = slot(focusedPane)
		let sectionAbove = Self.sectionAbove[focusedPane].flatMap { (sectionControlCounts[$0] ?? 0) > 0 ? $0 : nil }
		guard slot.itemCount > 0 else {
			// An empty list: `k` still reaches the section above it.
			if step < 0, entersSection, let sectionAbove { enter(sectionAbove) }
			return
		}
		// From the container, `j` lands on the first item and `k` on the last.
		if slot.isOnContainer {
			slot.isOnContainer = false
			slot.selection = step > 0 ? 0 : slot.itemCount - 1
			return
		}
		let current = min(slot.selection, slot.itemCount - 1)
		// Above the first item sits the pane's section, when it has one.
		if current == 0, step < 0, entersSection, let sectionAbove {
			enter(sectionAbove)
			return
		}
		let next = clamp(current + step, in: focusedPane)
		if slot.selection != next { slot.selection = next }
	}

	private func enter(_ section: FocusSection) {
		self.section = section
		let count = sectionControlCounts[section] ?? 0
		sectionControl = min(sectionControl, max(0, count - 1))
	}

	private func leaveSection() {
		if section != nil { section = nil }
	}

	/// Back from a section into its pane: `j` lands on the first row; Esc
	/// lands on the container under Vim motions and the first row under Standard.
	private func landBelowSection(in pane: Pane, isFromEsc: Bool) {
		let slot = slot(pane)
		if isFromEsc, scheme == .vim {
			slot.isOnContainer = true
			return
		}
		slot.isOnContainer = slot.itemCount == 0 && scheme == .vim
		slot.selection = 0
	}

	private func movePane(by step: Int) {
		let order = paneOrder
		guard let index = order.firstIndex(of: focusedPane) else { return }
		let target = order[min(max(index + step, 0), order.count - 1)]
		guard target != focusedPane else { return }
		focus(target)
	}

	private func setFocusedPane(_ pane: Pane) {
		if focusedPane != pane { focusedPane = pane }
	}

	private func slot(_ pane: Pane) -> PaneSlot {
		// Every pane has a slot from init; the dictionary is never mutated.
		slots[pane] ?? PaneSlot()
	}

	private func clamp(_ index: Int, in pane: Pane) -> Int {
		min(max(index, 0), max(0, slot(pane).itemCount - 1))
	}
}
