import Foundation

/// The localhost API over the app's own models: the same ``SessionModel``
/// the Now card, the menu-bar extra and the mini tracker use, so a session
/// started in one and stopped through the API is one session, logged once.
@MainActor
public final class AppTimerAPIBackend: TimerAPIBackend {
	private let sessions: SessionModel
	private let dayPlan: DayPlanModel
	/// The open workspace's name, for the answers.
	public var workspaceName: () -> String?
	/// The workload threshold the day's logged time is measured against.
	public var workloadMinutes: () -> Int
	private let now: () -> Date
	/// Called after an action changed the sessions or the vault, so the
	/// surfaces read them again.
	public var onChange: () -> Void = {}

	public init(
		sessions: SessionModel,
		dayPlan: DayPlanModel,
		workspaceName: @escaping () -> String? = { nil },
		workloadMinutes: @escaping () -> Int = { 8 * 60 },
		now: @escaping () -> Date = Date.init,
	) {
		self.sessions = sessions
		self.dayPlan = dayPlan
		self.workspaceName = workspaceName
		self.workloadMinutes = workloadMinutes
		self.now = now
	}

	public func status() -> TimerAPIStatus {
		status(logLine: nil)
	}

	public func today() -> TimerAPIToday {
		TimerAPIToday(
			workspace: workspaceName(),
			date: dayPlan.date,
			tasks: dayPlan.items.map { item in
				TimerAPITask(taskId: item.taskId, text: item.text, story: item.storyNumber, isDone: item.isDone, estimateMinutes: item.estimateMinutes)
			},
		)
	}

	public func startTimer(target: String, minutesAgo: Int?) -> Result<TimerAPIStatus, TimerAPIFailure> {
		guard sessions.isAvailable else { return .failure(Self.noWorkspace) }
		if let minutesAgo, !(1 ... 24 * 60).contains(minutesAgo) {
			return .failure(TimerAPIFailure(.invalid, "minutesAgo takes whole minutes from 1 to 1440"))
		}
		guard let resolved = TimerTargetResolver.resolve(target, items: dayPlan.items) else {
			return .failure(TimerAPIFailure(.notFound, "No open task of today matches \"\(target.trimmingCharacters(in: .whitespaces))\""))
		}
		if let open = sessions.openSession, open.taskId == resolved.taskId, resolved.taskId != nil {
			return .failure(TimerAPIFailure(.conflict, "The timer already runs on \(resolved.taskId ?? "")"))
		}
		return act(status: { self.status() }) {
			sessions.start(taskId: resolved.taskId, story: resolved.story, minutesAgo: minutesAgo)
		}
	}

	public func stopTimer() -> Result<TimerAPIStatus, TimerAPIFailure> {
		guard sessions.isAvailable else { return .failure(Self.noWorkspace) }
		guard sessions.openSession != nil else { return .failure(TimerAPIFailure(.conflict, "No timer is running")) }
		return act(status: { self.status(logLine: self.sessions.lastLogLine) }) { sessions.stop() }
	}

	public func continueTimer() -> Result<TimerAPIStatus, TimerAPIFailure> {
		guard sessions.isAvailable else { return .failure(Self.noWorkspace) }
		return act(status: { self.status() }) { sessions.continueLatest() }
	}

	public func completeTask(id: String) -> Result<TimerAPIDone, TimerAPIFailure> {
		guard sessions.isAvailable else { return .failure(Self.noWorkspace) }
		guard dayPlan.tick(taskId: id) else {
			let message = dayPlan.errorMessage ?? "Could not tick \(id)"
			let kind: TimerAPIFailure.Kind = message.lowercased().contains("no task") ? .notFound : .invalid
			return .failure(TimerAPIFailure(kind, message))
		}
		onChange()
		return .success(TimerAPIDone(taskId: id, isDone: true))
	}

	/// The text a surface shows for a session: its task's text, else the meeting, story or id.
	public func text(of session: SessionSummary) -> String {
		if let taskId = session.taskId, let text = taskText(taskId) { return text }
		return session.meetingTitle ?? session.story ?? session.taskId ?? "Timer"
	}

	private func act(status: () -> TimerAPIStatus, _ action: () -> Bool) -> Result<TimerAPIStatus, TimerAPIFailure> {
		let didRun = action()
		onChange()
		guard didRun else {
			let failure = sessions.lastFailure
			return .failure(TimerAPIFailure(Self.kind(of: failure?.kind), failure?.message ?? "The timer refused"))
		}
		return .success(status())
	}

	private func status(logLine: String?) -> TimerAPIStatus {
		let today = DayText.string(now())
		let dayTime = sessions.dayTime(on: today)
		let timer = sessions.openSession.map { session in
			TimerAPITimer(
				sessionId: session.id,
				taskId: session.taskId,
				story: session.story,
				text: text(of: session),
				isPaused: session.isPaused,
				startedAt: session.startedAt.map { ISO8601DateFormatter().string(from: $0) } ?? "",
				elapsedSeconds: session.elapsed(at: now()),
			)
		}
		return TimerAPIStatus(
			workspace: workspaceName(),
			timer: timer,
			day: TimerAPIDay(
				date: today,
				spentSeconds: dayTime?.spentSeconds ?? 0,
				loggedSeconds: dayTime?.loggedSeconds ?? 0,
				workloadSeconds: workloadMinutes() * 60,
			),
			logLine: logLine,
		)
	}

	private func taskText(_ taskId: String) -> String? {
		for item in dayPlan.items {
			if item.taskId == taskId { return item.text }
			if let subtask = item.subtasks.first(where: { $0.taskId == taskId }) { return subtask.text }
		}
		return nil
	}

	private static let noWorkspace = TimerAPIFailure(.unavailable, "No workspace is open in Vaultday")

	private static func kind(of kind: SessionFailureKind?) -> TimerAPIFailure.Kind {
		switch kind {
		case .conflict: .conflict
		case .invalidInput: .invalid
		case .noWorkspace: .unavailable
		case .other, nil: .invalid
		}
	}
}

/// What a typed timer target names: a task, or a story without a task.
public struct TimerTarget: Equatable, Sendable {
	public let taskId: String?
	public let story: String?
}

/// Reads `vaultday timer start <task-id|text>` and the menu bar's typed
/// entry: a task id of the day's list or `task-N`, a story number of the
/// list, else the first open task whose text holds the words.
public enum TimerTargetResolver {
	public static func resolve(_ typed: String, items: [PlanItem]) -> TimerTarget? {
		let target = typed.trimmingCharacters(in: .whitespacesAndNewlines)
		guard !target.isEmpty else { return nil }
		for item in items {
			if item.taskId == target { return TimerTarget(taskId: item.taskId, story: item.storyNumber) }
			if item.subtasks.contains(where: { $0.taskId == target }) { return TimerTarget(taskId: target, story: item.storyNumber) }
		}
		if isTaskId(target) { return TimerTarget(taskId: target, story: nil) }
		if let story = items.compactMap(\.storyNumber).first(where: { $0.caseInsensitiveCompare(target) == .orderedSame }) {
			return TimerTarget(taskId: nil, story: story)
		}
		let words = target.lowercased().split(separator: " ")
		let match = items.first { item in
			!item.isDone && words.allSatisfy { item.text.lowercased().contains($0) }
		}
		return match.map { TimerTarget(taskId: $0.taskId, story: $0.storyNumber) }
	}

	/// `task-118`: the ids the writer mints.
	static func isTaskId(_ text: String) -> Bool {
		guard text.hasPrefix("task-") else { return false }
		let number = text.dropFirst("task-".count)
		return !number.isEmpty && number.allSatisfy(\.isNumber)
	}
}
