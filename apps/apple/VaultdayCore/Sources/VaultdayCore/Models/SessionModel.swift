import Foundation
import Observation

/// One timer session, as the views see it.
public struct SessionSummary: Identifiable, Hashable, Sendable {
	public let id: String
	public let taskId: String?
	public let story: String?
	public let meetingTitle: String?
	public let startedAt: Date?
	public let stoppedAt: Date?
	public let isPaused: Bool
	/// Time counted when the session was read, pauses cut out.
	public let elapsedSeconds: Int
	/// When the session was read; a running timer counts on from here.
	public let readAt: Date

	/// Running or paused: not stopped yet.
	public var isOpen: Bool { stoppedAt == nil }

	/// The time counted at `now`, for the Now card's clock.
	public func elapsed(at now: Date) -> Int {
		guard isOpen, !isPaused else { return elapsedSeconds }
		return elapsedSeconds + max(0, Int(now.timeIntervalSince(readAt)))
	}
}

/// Why a timer action did not run, so the localhost API can answer with the
/// right status: a session already runs or none does, the input was refused,
/// or no workspace is open.
public enum SessionFailureKind: Sendable, Equatable {
	case conflict
	case invalidInput
	case noWorkspace
	case other
}

/// A refused timer action and the core's message for it.
public struct SessionFailure: Sendable, Equatable {
	public let kind: SessionFailureKind
	public let message: String
}

/// One day's timer time, the two kinds kept apart.
public struct SessionDayTime: Sendable, Equatable {
	public let spentSeconds: Int
	public let loggedSeconds: Int
}

/// Where the long-timer check's question goes: a user notification in the
/// app, a recorder in tests.
@MainActor
public protocol LongTimerAlerting: AnyObject {
	/// The session `sessionId` has counted `elapsedSeconds`: ask whether you
	/// are still working on it.
	func askStillWorking(sessionId: String, elapsedSeconds: Int)
}

/// The workspace's timer sessions from its synced app state, and the timer
/// controls every surface shares: the Now card, the menu-bar extra, the mini
/// tracker and the localhost API all call these, so one running session is
/// the one state they show. Stop writes the session's day-log line through
/// the core's writer, once per session.
@MainActor
@Observable
public final class SessionModel {
	public private(set) var sessions: [SessionSummary] = []
	public private(set) var errorMessage: String?
	/// Why the last action was refused; `nil` after one that ran.
	public private(set) var lastFailure: SessionFailure?
	/// The `session` line the last stop appended to the day log.
	public private(set) var lastLogLine: String?
	/// Why the last stopped session did not reach a time sink (the Toggl
	/// queue); the stop itself stands.
	public private(set) var timeSinkMessage: String?

	private var state: WorkspaceStateHandle?
	private var vault: WorkspaceHandle?
	private weak var workspace: WorkspaceModel?
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	/// The session still open, if any.
	public var openSession: SessionSummary? {
		sessions.first(where: \.isOpen)
	}

	/// The timer session recorded last, which "Continue latest" restarts; meetings do not count.
	public var latestTimerSession: SessionSummary? {
		sessions.first { $0.meetingTitle == nil }
	}

	/// Task ids the timer ran on, most recent first, each once.
	public func recentTaskIds(limit: Int) -> [String] {
		var seen: Set<String> = []
		var recent: [String] = []
		for taskId in sessions.compactMap(\.taskId) where !seen.contains(taskId) {
			seen.insert(taskId)
			recent.append(taskId)
			if recent.count == limit { break }
		}
		return recent
	}

	/// Whether a workspace's app state is loaded, so the timer can run.
	public var isAvailable: Bool { state != nil }

	/// Reads the sessions of the open workspace.
	public func load(from workspace: WorkspaceModel) {
		state = workspace.state
		vault = workspace.vault
		self.workspace = workspace
		reload()
	}

	/// Reads the sessions again, for a surface that shows them after another changed them.
	public func refresh() {
		reload()
	}

	/// Starts the timer on a task (or a story when `taskId` is `nil`); a
	/// running one is stopped and logged first, as dropping a new task on the
	/// Now card switches what it counts. With `minutesAgo`, "Start minutes
	/// ago": the session begins that long before now. `false` when refused.
	@discardableResult
	public func start(taskId: String?, story: String?, minutesAgo: Int? = nil) -> Bool {
		guard let state else { return refuseWithoutWorkspace() }
		return run {
			if openSession != nil {
				try stopAndLog(state)
			}
			if let minutesAgo {
				_ = try state.startSessionMinutesAgo(task: taskId, story: story, minutes: UInt32(clamping: max(1, minutesAgo)))
			} else {
				_ = try state.startSession(task: taskId, story: story)
			}
		}
	}

	/// "Continue latest": a new session on the target of the last timer session.
	@discardableResult
	public func continueLatest() -> Bool {
		guard let state else { return refuseWithoutWorkspace() }
		return run { _ = try state.continueLatestSession() }
	}

	/// Pause, or resume a paused session. Either way it stays one session.
	@discardableResult
	public func togglePause() -> Bool {
		guard let state, let session = openSession else { return refuseWithoutSession() }
		return run {
			_ = session.isPaused ? try state.resumeSession() : try state.pauseSession()
		}
	}

	/// Stops the open session and appends its `session` line to the day log.
	@discardableResult
	public func stop() -> Bool {
		guard let state, openSession != nil else { return refuseWithoutSession() }
		return run { try stopAndLog(state) }
	}

	/// The timer time inside the host's local `date` (`YYYY-MM-DD`), up to now.
	public func dayTime(on date: String) -> SessionDayTime? {
		guard let view = try? state?.dayTime(date: date) else { return nil }
		return SessionDayTime(spentSeconds: Int(view.spentSeconds), loggedSeconds: Int(view.loggedSeconds))
	}

	/// The long-timer check: once per session, when the open session has
	/// counted `limitMinutes`, `alert` asks whether you are still working on
	/// it. Returns whether it asked this time.
	@discardableResult
	public func checkLongTimer(limitMinutes: Int, alert: any LongTimerAlerting) -> Bool {
		guard let state, openSession != nil, limitMinutes > 0 else { return false }
		let port = LongTimerPort()
		let didFire: Bool
		do {
			didFire = try state.checkLongTimer(limitMinutes: UInt32(clamping: limitMinutes), notifier: port)
			_ = try state.flush()
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
		for (sessionId, elapsed) in port.takeLongTimers() {
			alert.askStillWorking(sessionId: sessionId, elapsedSeconds: elapsed)
		}
		reload()
		return didFire
	}

	private func stopAndLog(_ state: WorkspaceStateHandle) throws {
		guard let vault else {
			let stopped = try state.stopSession()
			lastLogLine = nil
			sendToTimeSinks(sessionId: stopped.id)
			return
		}
		let stopped = try state.stopAndLogSession(workspace: vault)
		lastLogLine = stopped.logLine
		sendToTimeSinks(sessionId: stopped.session.id)
		if let logError = stopped.logError {
			throw SessionLogError(message: "The session stopped, but its day-log line was not written: \(logError)")
		}
	}

	/// The stop stands whatever a time sink answers; its failure is only reported.
	private func sendToTimeSinks(sessionId: String) {
		do {
			try workspace?.sendToTimeSinks(sessionId: sessionId)
			timeSinkMessage = nil
		} catch {
			timeSinkMessage = coreErrorMessage(error)
		}
	}

	private func refuseWithoutWorkspace() -> Bool {
		fail(SessionFailure(kind: .noWorkspace, message: "The timer needs the workspace's app state"))
		return false
	}

	private func refuseWithoutSession() -> Bool {
		fail(SessionFailure(kind: .conflict, message: "No timer is running"))
		return false
	}

	private func fail(_ failure: SessionFailure) {
		lastFailure = failure
		errorMessage = failure.message
	}

	/// Runs the edits and writes what they changed, even when a later step
	/// failed after an earlier one was applied.
	private func run(_ body: () throws -> Void) -> Bool {
		var didSucceed = true
		do {
			try body()
			errorMessage = nil
			lastFailure = nil
		} catch {
			fail(SessionFailure(kind: failureKind(of: error), message: coreErrorMessage(error)))
			didSucceed = false
		}
		do {
			_ = try state?.flush()
		} catch {
			fail(SessionFailure(kind: .other, message: coreErrorMessage(error)))
			didSucceed = false
		}
		reload()
		return didSucceed
	}

	private func reload() {
		guard let state else {
			sessions = []
			return
		}
		let readAt = now()
		sessions = state.sessions()
			.map { mapSession($0, readAt: readAt) }
			.sorted { ($0.startedAt ?? .distantPast) > ($1.startedAt ?? .distantPast) }
	}
}

/// A stop whose day-log line failed; the session itself is stopped.
private struct SessionLogError: LocalizedError {
	let message: String
	var errorDescription: String? { message }
}

private func failureKind(of error: any Error) -> SessionFailureKind {
	guard let error = error as? CoreError else { return .other }
	switch error {
	case .Conflict: return .conflict
	case .InvalidInput, .InvalidDate, .NotFound: return .invalidInput
	default: return .other
	}
}

/// The core's `Notifier` port for one check: it keeps what the core reported
/// so the alert runs on the main actor after the core call returns.
private final class LongTimerPort: NotifierPort, @unchecked Sendable {
	// Only touched during the synchronous `checkLongTimer` call and right after it.
	private let lock = NSLock()
	private var longTimers: [(String, Int)] = []

	func longTimer(sessionId: String, elapsedSeconds: UInt64) {
		lock.withLock { longTimers.append((sessionId, Int(clamping: elapsedSeconds))) }
	}

	func thresholdCrossed(task _: String, story _: String?, reportedSeconds _: UInt64, estimateSeconds _: UInt64) {}

	func newSignals(workspace _: String, added _: UInt32, waiting _: UInt32) {}

	func takeLongTimers() -> [(String, Int)] {
		lock.withLock {
			defer { longTimers = [] }
			return longTimers
		}
	}
}

private func mapSession(_ session: SessionView, readAt: Date) -> SessionSummary {
	SessionSummary(
		id: session.id,
		taskId: session.task,
		story: session.story,
		meetingTitle: session.meetingTitle,
		startedAt: parseTimestamp(session.startedAt),
		stoppedAt: session.stoppedAt.flatMap(parseTimestamp),
		isPaused: session.isPaused,
		elapsedSeconds: Int(session.elapsedSeconds),
		readAt: readAt,
	)
}

private func parseTimestamp(_ text: String) -> Date? {
	let formatter = ISO8601DateFormatter()
	formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
	return formatter.date(from: text) ?? ISO8601DateFormatter().date(from: text)
}
