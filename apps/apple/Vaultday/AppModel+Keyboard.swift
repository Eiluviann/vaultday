import AppKit
import Carbon.HIToolbox
import os
import VaultdayCore

/// A row of the sidebar the keys walk with `j` `k`, top to bottom.
enum SidebarRow: Hashable {
	case allWorkspaces
	case workspace(String)
	/// The Now card: ⏎ pauses or resumes the running timer, or continues the latest.
	case now
	/// A ritual: highlighted, inert until rituals run.
	case ritual(String)
	case browse
	/// The health dot in the footer: ⏎ opens its list.
	case health
	case settings
}

/// A control of the Today header, left to right.
enum DayHeaderControl: Hashable {
	case today
	case previousDay
	case nextDay
	/// The List / 3 days / Week switch: ⏎ steps to the next span.
	case span
	case autoSchedule
	case replan
}

extension AppModel {
	// MARK: Sidebar rows

	/// The sidebar's rows in the order `j` and `k` walk them.
	var sidebarRows: [SidebarRow] {
		var rows: [SidebarRow] = [.allWorkspaces]
		rows += workspaceEntries.filter { screenShare.canShow($0) }.map { .workspace($0.id) }
		rows.append(.now)
		rows += ritualRows(now: Date()).map { .ritual($0.id) }
		rows += [.browse, .health, .settings]
		return rows
	}

	/// The sidebar told the focus model what it lists; the current row
	/// follows the open workspace, so `g1` lands there.
	func syncSidebarRows(_ rows: [SidebarRow]) {
		focus.setItemCount(rows.count, in: .sidebar)
		let current: SidebarRow = isShowingAll ? .allWorkspaces : workspace.map { .workspace($0.workspaceId) } ?? .allWorkspaces
		guard current != lastSidebarWorkspaceRow, let index = rows.firstIndex(of: current) else { return }
		lastSidebarWorkspaceRow = current
		focus.setCurrentRow(index, in: .sidebar)
	}

	/// Whether the keyboard highlight is on `row`.
	func isHighlighted(_ row: SidebarRow) -> Bool {
		guard focus.focusedPane == .sidebar, let index = focus.selection(in: .sidebar) else { return false }
		let rows = sidebarRows
		return rows.indices.contains(index) && rows[index] == row
	}

	/// ⏎ on a sidebar row.
	func activateSidebarRow(at index: Int) {
		let rows = sidebarRows
		guard rows.indices.contains(index) else { return }
		switch rows[index] {
		case .allWorkspaces:
			showAllWorkspaces()
		case let .workspace(id):
			if let entry = workspaceEntries.first(where: { $0.id == id }) { showWorkspace(entry) }
		case .now:
			if sessions.openSession != nil {
				togglePauseTimer()
			} else if sessions.latestTimerSession != nil {
				continueLatestTimer()
			}
		case .ritual:
			break
		case .browse:
			showBrowse()
		case .health:
			isShowingHealth.toggle()
		case .settings:
			toggleSettings()
		}
	}

	// MARK: The Today header

	/// The header told the focus model which controls it shows.
	func setDayHeaderControls(_ controls: [DayHeaderControl]) {
		dayHeaderControls = controls
		focus.setControlCount(controls.count, in: .dayHeader)
	}

	/// Whether the keyboard highlight is on `control` of the header.
	func isHighlighted(_ control: DayHeaderControl) -> Bool {
		guard let index = focus.control(in: .dayHeader), dayHeaderControls.indices.contains(index) else { return false }
		return dayHeaderControls[index] == control
	}

	/// ⏎ on the header's highlighted control: what a click on it does.
	func activateDayHeaderControl() {
		guard let index = focus.control(in: .dayHeader), dayHeaderControls.indices.contains(index) else { return }
		switch dayHeaderControls[index] {
		case .today:
			perform(.showToday)
		case .previousDay:
			perform(.previousDay)
		case .nextDay:
			perform(.nextDay)
		case .span:
			let spans = CalendarSpan.allCases
			let next = spans.firstIndex(of: calendarPanel.span).map { spans[($0 + 1) % spans.count] } ?? .list
			showSpan(next)
		case .autoSchedule:
			perform(.autoSchedule)
		case .replan:
			perform(.replan)
		}
	}

	// MARK: The setup flow

	/// A key in the setup flow under Vim motions: `h` `j` `k` `l` walk the
	/// step list and the shown step's controls, ⏎ presses the highlighted
	/// control or continues, Esc drops the highlight. `false` leaves the key
	/// to AppKit (Standard, a field being typed in, ⌘ keys).
	func handleSetupKey(_ stroke: KeyStroke, isTyping: Bool) -> Bool {
		guard dispatcher.scheme == .vim, !isTyping, stroke.modifiers.isDisjoint(with: [.command, .control]), let onboarding = setup?.onboarding else {
			return false
		}
		let steps = SetupStep.allCases
		setupKeyboard.setSteps(count: steps.count, reachable: onboarding.furthest.rawValue, current: onboarding.current.rawValue - 1)
		switch dispatcher.handle(stroke) {
		case .pending, .cancelled:
			return true
		case .unbound:
			return false
		case let .action(action):
			switch setupKeyboard.apply(action) {
			case .none:
				return [.moveDown, .moveUp, .moveLeft, .moveRight, .moveFirst, .moveLast, .dismiss].contains(action)
			case let .activate(id):
				SetupOptionActions.shared.run(id)
			case let .openStep(index):
				if steps.indices.contains(index) { _ = onboarding.jump(to: steps[index]) }
			case .continueSetup:
				if onboarding.isLastStep { finishSetup() } else { onboarding.goForward() }
			}
			return true
		}
	}

	// MARK: Text inputs in the focus tree

	/// The pane each Home text input belongs to: Esc in it comes back there.
	/// The palette is a layer over the window, not an input of a pane.
	static func pane(ofTextField name: String) -> Pane? {
		switch name {
		case "quick-add", "adjust", "browse-query", "preset-name", "change": .today
		case "day-log-append": .sidePanel
		case "signal-draft": .inbox
		default: nil
		}
	}

	// MARK: ⌘ and the key left of 1

	/// The key left of 1 on this Mac's keyboard: § on an ISO keyboard, ` on ANSI and JIS.
	static var leftOfOneKeyCode: UInt16 {
		KBGetLayoutType(Int16(LMGetKbdType())) == kKeyboardISO ? UInt16(kVK_ISO_Section) : UInt16(kVK_ANSI_Grave)
	}

	/// The character the cap of that key shows.
	static var leftOfOneCap: String {
		leftOfOneKeyCode == UInt16(kVK_ISO_Section) ? "§" : "`"
	}

	/// The stroke a key event is, by key code for the key left of 1, so the
	/// layout's character for it does not matter.
	static func leftOfOneStroke(_ event: NSEvent) -> KeyStroke? {
		guard event.keyCode == leftOfOneKeyCode else { return nil }
		var modifiers: KeyModifiers = []
		let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
		if flags.contains(.command) { modifiers.insert(.command) }
		if flags.contains(.shift) { modifiers.insert(.shift) }
		if flags.contains(.control) { modifiers.insert(.control) }
		if flags.contains(.option) { modifiers.insert(.option) }
		return KeyStroke(key: KeyStroke.leftOfOneKey, modifiers: modifiers)
	}
}

/// Debug only: how long a key takes from its event to the Core Animation
/// commit that paints its effect, as an `os_signpost` interval and a log
/// line (`log stream --predicate 'subsystem == "com.eiluviann.vaultday" && category == "keys"'`).
@MainActor
enum KeyLatency {
	#if DEBUG
		private static let signposter = OSSignposter(subsystem: "com.eiluviann.vaultday", category: "keys")
		private static let logger = Logger(subsystem: "com.eiluviann.vaultday", category: "keys")
	#endif

	/// Marks a handled key whose handler started at `handlerStart` (system
	/// uptime); the interval ends after the next commit.
	static func mark(_ stroke: KeyStroke, event: NSEvent?, handlerStart: TimeInterval) {
		#if DEBUG
			let eventTime = event?.timestamp ?? handlerStart
			let handledTime = handlerStart
			let state = signposter.beginInterval("key", id: signposter.makeSignpostID(), "\(stroke.description, privacy: .public)")
			// Core Animation commits in the run loop's before-waiting observer at
			// order 2,000,000; an observer after it sees the commit done.
			let observer = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, false, 2_000_001) { _, _ in
				let committed = ProcessInfo.processInfo.systemUptime
				signposter.endInterval("key", state)
				let total = (committed - eventTime) * 1000
				let inApp = (committed - handledTime) * 1000
				logger.notice("key \(stroke.description, privacy: .public): \(String(format: "%.1f", total), privacy: .public) ms event to commit, \(String(format: "%.1f", inApp), privacy: .public) ms handler to commit")
			}
			CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
		#endif
	}
}
