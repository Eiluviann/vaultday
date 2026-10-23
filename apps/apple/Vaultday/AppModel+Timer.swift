import AppKit
import SwiftUI
import VaultdayCore

/// The timer surfaces outside the Home window: the menu-bar extra, the mini
/// tracker, the localhost API and the long-timer check. Every one acts on
/// the same `SessionModel`, so they show one running session.
extension AppModel {
	/// How often the long-timer check runs, in clock ticks of a second.
	private static let longTimerCheckInterval = 30

	/// Starts the localhost API, the one-second clock, the ⌥⌘T hotkey and the
	/// watch on which window is frontmost.
	func startTimerSurfaces(environment: CoreEnvironment) {
		let api = LocalAPIService(environment: environment, backend: timerBackend)
		localAPI = api
		Task { await api.start() }
		longTimerAlert = UserNotificationLongTimerAlert { [weak self] sessionId in
			guard let self, let session = sessions.sessions.first(where: { $0.id == sessionId }) else { return nil }
			return timerBackend.text(of: session)
		}
		registerTrackerHotKey()
		observeFrontmostWindow()
		refreshTimerPicker()
		timerSurfacesTask = Task { [weak self] in
			var tick = 0
			while !Task.isCancelled {
				try? await Task.sleep(for: .seconds(1))
				guard let self else { return }
				tick += 1
				if sessions.openSession != nil { timerClock = Date() }
				if tick % Self.longTimerCheckInterval == 0 { checkLongTimer() }
			}
		}
		miniTracker.attach(app: self)
	}

	/// Called once from `init`: the API answers with the open workspace's name and workload.
	func configureTimerBackend() {
		timerBackend.workspaceName = { [weak self] in self?.workspace?.name }
		timerBackend.workloadMinutes = { [weak self] in self?.rhythm?.workloadThresholdMinutes ?? 8 * 60 }
		timerBackend.onChange = { [weak self] in self?.timerChanged() }
	}

	/// The open session, as every surface shows it.
	var runningSession: SessionSummary? { sessions.openSession }

	/// The running session's text: its task, else the meeting, story or id.
	func sessionText(_ session: SessionSummary) -> String {
		timerBackend.text(of: session)
	}

	/// The task after the running one in the day's list, for the expanded tracker.
	var nextUpItem: PlanItem? {
		let unfinished = dayPlan.unfinishedItems
		guard let taskId = runningSession?.taskId, let index = unfinished.firstIndex(where: { $0.taskId == taskId }) else {
			return unfinished.first
		}
		return unfinished.dropFirst(index + 1).first
	}

	/// The running task's list item, for its estimate and subtasks.
	var runningItem: PlanItem? {
		guard let taskId = runningSession?.taskId else { return nil }
		return dayPlan.items.first { $0.taskId == taskId }
	}

	/// Logged against the workload threshold, 0 to 1, for the menu bar's ring.
	var loggedFraction: Double {
		let workload = rhythm?.workloadThresholdMinutes ?? 8 * 60
		guard workload > 0 else { return 0 }
		return min(1, Double(dayPlan.totals.loggedMinutes) / Double(workload))
	}

	/// "3h 10m of 8h logged".
	var loggedSummary: String {
		"\(DurationText.minutes(dayPlan.totals.loggedMinutes)) of \(DurationText.minutes(rhythm?.workloadThresholdMinutes ?? 8 * 60)) logged"
	}

	/// The menu bar's title: the running clock, or nothing.
	var menuBarClock: String? {
		guard let session = runningSession, !screenShare.hides(.menuBarTitle) else { return nil }
		return DurationText.clock(seconds: session.elapsed(at: timerClock))
	}

	func togglePauseTimer() {
		report(sessions.togglePause())
	}

	/// The mini tracker's global hotkey: the first single-stroke binding of
	/// `tracker.toggle` in Keymap.yaml under the current scheme (⌥⌘T), so
	/// the keymap stays the one source of keys. Called again when the
	/// scheme changes; nothing registers when the keymap binds no chord.
	func registerTrackerHotKey() {
		hotKey?.unregister()
		hotKey = nil
		guard let stroke = dispatcher.keymap.singleStroke(for: .toggleMiniTracker, scheme: dispatcher.scheme),
		      let chord = HotKeyChord(stroke)
		else { return }
		hotKey = GlobalHotKey(keyCode: chord.keyCode, modifiers: chord.carbonModifiers) { [weak self] in
			self?.toggleMiniTracker()
		}
	}

	/// Stop: ends the session at once; its day-log line is written off the
	/// main actor, and the surfaces read the day again once it is.
	func stopTimer() {
		report(sessions.stop())
		Task { [weak self] in
			await self?.sessions.waitForDayLog()
			guard let self else { return }
			if let message = sessions.errorMessage { timerMessage = message }
			timerChanged()
		}
	}

	/// "Continue latest": a new session on the last timer's task.
	func continueLatestTimer() {
		report(sessions.continueLatest())
	}

	/// The menu bar's typed entry (⌘N): a task id, a story number or text
	/// of an open task, matched as `vaultday timer start` matches it.
	func startTypedTimer(_ text: String, minutesAgo: Int? = nil) {
		switch timerBackend.startTimer(target: text, minutesAgo: minutesAgo) {
		case .success:
			timerMessage = nil
			timerPicker.endNewEntry()
		case let .failure(failure):
			timerMessage = failure.message
		}
	}

	/// ⏎ in the menu-bar extra: the typed entry, or the selected row.
	func startPickedTimer(minutesAgo: Int? = nil) {
		if timerPicker.isComposing {
			startTypedTimer(timerPicker.draft, minutesAgo: minutesAgo)
		} else if let pick = timerPicker.selectedPick {
			startTimer(taskId: pick.taskId, minutesAgo: minutesAgo)
		}
	}

	/// The expanded tracker's Done: stops the timer and ticks the task.
	func finishRunningTask() {
		guard let taskId = runningSession?.taskId else { return }
		stopTimer()
		completeTask(taskId)
	}

	/// ⌥⌘T: shows the tracker when it was hidden, else folds or unfolds it.
	func toggleMiniTracker() {
		if isMiniTrackerEnabled {
			isMiniTrackerExpanded.toggle()
		} else {
			isMiniTrackerEnabled = true
		}
	}

	func hideMiniTracker() {
		isMiniTrackerEnabled = false
		isMiniTrackerExpanded = false
	}

	/// Whether the mini tracker shows now.
	var isMiniTrackerVisible: Bool {
		MiniTrackerLayout.isVisible(hasOpenSession: runningSession != nil, isEnabled: isMiniTrackerEnabled, isMainWindowFrontmost: isMainWindowFrontmost)
	}

	/// Brings the Home window forward, from the menu bar or the tracker.
	func openMainWindow(taskId: String? = nil) {
		NSApp.activate()
		if let window = NSApp.windows.first(where: { $0.identifier?.rawValue.hasPrefix("home") == true }) {
			window.makeKeyAndOrderFront(nil)
		}
		if let taskId { openPreview(taskId: taskId) }
	}

	/// The sessions or the vault changed through a surface or the API: the
	/// day, the hours and the menu bar's list read them again.
	func timerChanged() {
		sessions.refresh()
		dayPlan.reload()
		syncTodayItemCount()
		if let workspace, workspace.phase == .open, !dayPlan.date.isEmpty {
			hours.load(from: workspace, date: dayPlan.date)
		}
		refreshTimerPicker()
		timerClock = Date()
	}

	func refreshTimerPicker() {
		timerPicker.load(recentTaskIds: sessions.recentTaskIds(limit: 5), items: dayPlan.items)
	}

	private func checkLongTimer() {
		guard let longTimerAlert else { return }
		sessions.checkLongTimer(limitMinutes: rhythm?.longTimerCheckMinutes ?? 3 * 60, alert: longTimerAlert)
	}

	private func report(_ didRun: Bool) {
		timerMessage = didRun ? nil : sessions.errorMessage
		timerChanged()
	}

	private func observeFrontmostWindow() {
		let center = NotificationCenter.default
		let names: [Notification.Name] = [
			NSApplication.didBecomeActiveNotification,
			NSApplication.didResignActiveNotification,
			NSWindow.didBecomeKeyNotification,
			NSWindow.didResignKeyNotification,
		]
		windowObservers = names.map { name in
			center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
				MainActor.assumeIsolated { self?.updateMainWindowFrontmost() }
			}
		}
		windowObservers.append(center.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { [weak self] _ in
			MainActor.assumeIsolated { self?.localAPI?.stop() }
		})
		updateMainWindowFrontmost()
	}

	private func updateMainWindowFrontmost() {
		let isHomeKey = NSApp.keyWindow?.identifier?.rawValue.hasPrefix("home") == true
		isMainWindowFrontmost = NSApp.isActive && isHomeKey
	}
}
