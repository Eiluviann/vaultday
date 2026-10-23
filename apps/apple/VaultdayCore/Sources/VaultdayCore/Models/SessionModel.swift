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

/// The workspace's timer sessions from its synced app state, and the Now
/// card's controls: start (a task dropped on the card), pause, resume, stop.
/// Writing the session's day-log line on stop arrives with the timer surfaces.
@MainActor
@Observable
public final class SessionModel {
	public private(set) var sessions: [SessionSummary] = []
	public private(set) var errorMessage: String?

	private var state: WorkspaceStateHandle?
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	/// The session still open, if any.
	public var openSession: SessionSummary? {
		sessions.first(where: \.isOpen)
	}

	/// Reads the sessions of the open workspace.
	public func load(from workspace: WorkspaceModel) {
		state = workspace.state
		reload()
	}

	/// Starts the timer on a task; a running one is stopped first, as dropping
	/// a new task on the Now card switches what it counts.
	public func start(taskId: String, story: String?) {
		guard let state else {
			errorMessage = "The timer needs the workspace's app state"
			return
		}
		run {
			if openSession != nil {
				_ = try state.stopSession()
			}
			_ = try state.startSession(task: taskId, story: story)
		}
	}

	/// Pause, or resume a paused session.
	public func togglePause() {
		guard let state, let session = openSession else { return }
		run {
			_ = session.isPaused ? try state.resumeSession() : try state.pauseSession()
		}
	}

	public func stop() {
		guard let state, openSession != nil else { return }
		run { _ = try state.stopSession() }
	}

	/// Runs the edits and writes what they changed, even when a later step
	/// failed after an earlier one was applied.
	private func run(_ body: () throws -> Void) {
		do {
			try body()
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		do {
			_ = try state?.flush()
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reload()
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
