import Foundation

/// The Toggl feature of one open workspace over the core: the week and the
/// queue for the Hours slots and Settings, and the time sink a stopped
/// session goes to. The core asks the registry first on every call, so with
/// the feature off nothing of it runs.
final class CoreToggl: TogglWeekSource, TogglQueueSource, @unchecked Sendable {
	private let appState: AppStateHandle
	private let state: WorkspaceStateHandle
	private let vault: WorkspaceHandle?
	private let workspaceId: String

	init(appState: AppStateHandle, state: WorkspaceStateHandle, vault: WorkspaceHandle?, workspaceId: String) {
		self.appState = appState
		self.state = state
		self.vault = vault
		self.workspaceId = workspaceId
	}

	func togglWeek(startingOn weekStart: String, story: String?) throws -> TogglWeekSnapshot {
		let view = try VaultdayCore.togglWeek(app: appState, state: state, workspaceId: workspaceId, weekStart: weekStart, story: story)
		return TogglWeekSnapshot(isConnected: view.isEnabled, days: view.days.map(TogglWeekDay.init))
	}

	func togglQueue() throws -> [TogglQueueEntry] {
		try VaultdayCore.togglQueue(app: appState, state: state, workspaceId: workspaceId).map(TogglQueueEntry.init)
	}

	func drainTogglQueue() throws -> TogglDrainOutcome {
		let view = try VaultdayCore.drainTogglQueue(app: appState, state: state, workspaceId: workspaceId)
		_ = try state.flush()
		return TogglDrainOutcome(
			notConnected: view.notConnected,
			sent: Int(view.sent),
			retrying: Int(view.retrying),
			failed: Int(view.failed),
			isUnauthorized: view.isUnauthorized,
		)
	}

	/// Queues a stopped session as its workspace's billing model says;
	/// nothing while the feature is off. Returns the number of entries queued.
	@discardableResult
	func enqueue(sessionId: String) throws -> Int {
		guard let vault else { return 0 }
		let outcome = try enqueueTogglSession(app: appState, state: state, workspace: vault, workspaceId: workspaceId, sessionId: sessionId)
		guard case let .queued(entries) = outcome else { return 0 }
		_ = try state.flush()
		return Int(entries)
	}
}

extension TogglWeekDay {
	init(_ view: TogglDayView) {
		let state: State = switch view.state {
		case .pushed: .pushed
		case .queued: .queued
		case .failed: .failed
		case .empty: .empty
		}
		self.init(date: view.date, weekday: Self.weekday(of: view.date), seconds: Int(view.seconds), isFlat: view.isFlat, state: state)
	}

	/// "Mon" for `2026-10-19`.
	static func weekday(of day: String) -> String {
		guard let date = DayText.date(from: day) else { return day }
		return weekdayFormatter.string(from: date)
	}

	private static let weekdayFormatter: DateFormatter = {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_GB")
		formatter.dateFormat = "EEE"
		return formatter
	}()
}

extension TogglQueueEntry {
	init(_ view: TogglQueueItemView) {
		let state: State = switch view.state {
		case .queued: .queued
		case .sent: .sent
		case .failed: .failed
		}
		self.init(
			key: view.key,
			description: view.description,
			story: view.story,
			durationSeconds: Int(view.durationSeconds),
			isBillable: view.isBillable,
			state: state,
			attempts: Int(view.attempts),
			lastError: view.lastError,
		)
	}
}

extension WorkspaceModel {
	/// The Toggl feature of the open workspace; `nil` until its app state is loaded.
	var toggl: CoreToggl? {
		guard let state, !workspaceId.isEmpty else { return nil }
		return CoreToggl(appState: session.appState, state: state, vault: vault, workspaceId: workspaceId)
	}

	/// The week source for the Hours slots.
	public var togglWeekSource: (any TogglWeekSource)? { toggl }

	/// The queue source for Settings and the drain scheduler.
	public var togglQueueSource: (any TogglQueueSource)? { toggl }

	/// Hands a stopped session to the time sinks the registry has on for this
	/// workspace (the Toggl queue). Every stop surface ends here; a session is
	/// queued once however often it arrives.
	func sendToTimeSinks(sessionId: String) throws {
		try toggl?.enqueue(sessionId: sessionId)
	}
}
