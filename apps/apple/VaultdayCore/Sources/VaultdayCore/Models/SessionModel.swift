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

	/// Running or paused: not stopped yet.
	public var isOpen: Bool { stoppedAt == nil }
}

/// The workspace's timer sessions from its synced app state. Read-only in
/// this shell; the timer surfaces arrive in PR 11.
@MainActor
@Observable
public final class SessionModel {
	public private(set) var sessions: [SessionSummary] = []

	public init() {}

	/// The session still open, if any.
	public var openSession: SessionSummary? {
		sessions.first(where: \.isOpen)
	}

	/// Reads the sessions of the open workspace.
	public func load(from workspace: WorkspaceModel) {
		guard let state = workspace.state else {
			sessions = []
			return
		}
		sessions = state.sessions().map(mapSession).sorted { ($0.startedAt ?? .distantPast) > ($1.startedAt ?? .distantPast) }
	}
}

private func mapSession(_ session: SessionView) -> SessionSummary {
	SessionSummary(
		id: session.id,
		taskId: session.task,
		story: session.story,
		meetingTitle: session.meetingTitle,
		startedAt: parseTimestamp(session.startedAt),
		stoppedAt: session.stoppedAt.flatMap(parseTimestamp),
	)
}

private func parseTimestamp(_ text: String) -> Date? {
	let formatter = ISO8601DateFormatter()
	formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
	return formatter.date(from: text) ?? ISO8601DateFormatter().date(from: text)
}
