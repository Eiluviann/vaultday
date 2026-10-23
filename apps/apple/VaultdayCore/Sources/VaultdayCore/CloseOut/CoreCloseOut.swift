import Foundation

/// Where a close-out comes from: the core in the app.
@MainActor
public protocol CloseOutSource: AnyObject {
	/// The period the header offers at `nowLocal` (`YYYY-MM-DDTHH:MM`, this Mac's time when `nil`).
	func due(nowLocal: String?) throws -> CloseOutDue?
	func closeOut(kind: CloseOutPeriodKind, date: String, draft: [String: [CloseOutAllotment]]) throws -> CloseOutSnapshot
	/// Queues the Toggl entries through the feature set; sends nothing. Returns how many.
	func queueToggl(kind: CloseOutPeriodKind, date: String) throws -> Int
	/// The draft after `story` on `day` was set to `minutes`.
	func rebalance(kind: CloseOutPeriodKind, date: String, draft: [String: [CloseOutAllotment]], day: String, story: String, minutes: Int) throws -> [String: [CloseOutAllotment]]
	func confirmAllocation(kind: CloseOutPeriodKind, date: String, days: [String: [CloseOutAllotment]]) throws
	func confirmReview(kind: CloseOutPeriodKind, date: String) throws
	func setTicked(kind: CloseOutPeriodKind, date: String, sink: String, key: String, isTicked: Bool) throws
	/// Writes the closing status line; returns it, `nil` when it was closed before.
	func close(kind: CloseOutPeriodKind, date: String, nowLocal: String?) throws -> String?
	/// The Monday alert: what the week before misses, once per week; `nil` when it does not fire.
	func checkAlert(nowLocal: String?) throws -> CloseOutAlert?
}

/// The Monday 07:30 alert.
public struct CloseOutAlert: Hashable, Sendable {
	public let workspaceName: String
	/// The week's Monday, `YYYY-MM-DD`.
	public let weekStart: String
	public let missing: [String]

	/// "Week of Mon 19 still open".
	public var title: String { "Week of \(CloseOutText.dayTitle(weekStart)) still open" }
}

/// One open workspace's close-out over the core. Every edit is flushed to
/// the workspace document, so ticks and confirmations survive a restart.
@MainActor
final class CoreCloseOut: CloseOutSource {
	private let appState: AppStateHandle
	private let state: WorkspaceStateHandle
	private let vault: WorkspaceHandle
	private let workspaceId: String
	private let workspaceName: String
	private let leaveRoot: String?

	init(appState: AppStateHandle, state: WorkspaceStateHandle, vault: WorkspaceHandle, workspaceId: String, workspaceName: String, leaveRoot: String?) {
		self.appState = appState
		self.state = state
		self.vault = vault
		self.workspaceId = workspaceId
		self.workspaceName = workspaceName
		self.leaveRoot = leaveRoot
	}

	func due(nowLocal: String?) throws -> CloseOutDue? {
		guard let view = try closeOutDue(app: appState, state: state, workspace: vault, workspaceId: workspaceId, leaveRoot: leaveRoot, nowLocal: nowLocal) else { return nil }
		return CloseOutDue(kind: CloseOutPeriodKind(view.kind), date: view.date, title: view.title, deadline: view.deadline, isOverdue: view.isOverdue)
	}

	func closeOut(kind: CloseOutPeriodKind, date: String, draft: [String: [CloseOutAllotment]]) throws -> CloseOutSnapshot {
		CloseOutSnapshot(try closeOutState(app: appState, state: state, workspace: vault, request: request(kind, date, draft: draft)))
	}

	func queueToggl(kind: CloseOutPeriodKind, date: String) throws -> Int {
		let queued = try closeOutQueueToggl(app: appState, state: state, workspace: vault, request: request(kind, date))
		_ = try state.flush()
		return Int(queued)
	}

	func rebalance(kind: CloseOutPeriodKind, date: String, draft: [String: [CloseOutAllotment]], day: String, story: String, minutes: Int) throws -> [String: [CloseOutAllotment]] {
		let views = try closeOutRebalance(app: appState, state: state, workspace: vault, request: request(kind, date, draft: draft), date: day, story: story, minutes: UInt32(clamping: max(0, minutes)))
		return Self.draft(views)
	}

	func confirmAllocation(kind: CloseOutPeriodKind, date: String, days: [String: [CloseOutAllotment]]) throws {
		try closeOutConfirmAllocation(app: appState, state: state, workspace: vault, request: request(kind, date), days: Self.views(days))
		_ = try state.flush()
	}

	func confirmReview(kind: CloseOutPeriodKind, date: String) throws {
		try closeOutConfirmReview(app: appState, state: state, workspace: vault, request: request(kind, date))
		_ = try state.flush()
	}

	func setTicked(kind: CloseOutPeriodKind, date: String, sink: String, key: String, isTicked: Bool) throws {
		try closeOutTick(app: appState, state: state, workspace: vault, request: request(kind, date), sink: sink, key: key, isTicked: isTicked)
		_ = try state.flush()
	}

	func close(kind: CloseOutPeriodKind, date: String, nowLocal: String?) throws -> String? {
		let closed = try closeOutClose(app: appState, state: state, workspace: vault, request: request(kind, date), nowLocal: nowLocal)
		_ = try state.flush()
		return closed.line
	}

	func checkAlert(nowLocal: String?) throws -> CloseOutAlert? {
		let port = CloseOutAlertPort()
		guard try closeOutCheckAlert(app: appState, state: state, workspace: vault, workspaceId: workspaceId, leaveRoot: leaveRoot, notifier: port, nowLocal: nowLocal) else { return nil }
		_ = try state.flush()
		guard let told = port.take() else { return nil }
		return CloseOutAlert(workspaceName: workspaceName, weekStart: told.weekStart, missing: told.missing)
	}

	private func request(_ kind: CloseOutPeriodKind, _ date: String, draft: [String: [CloseOutAllotment]] = [:]) -> CloseOutRequest {
		CloseOutRequest(workspaceId: workspaceId, kind: kind.core, date: date, leaveRoot: leaveRoot, draft: Self.views(draft))
	}

	private static func views(_ days: [String: [CloseOutAllotment]]) -> [DraftDayView] {
		days.keys.sorted().map { day in
			DraftDayView(date: day, allotments: (days[day] ?? []).map { AllotmentView(story: $0.story, minutes: UInt32(clamping: max(0, $0.minutes))) })
		}
	}

	private static func draft(_ views: [DraftDayView]) -> [String: [CloseOutAllotment]] {
		Dictionary(uniqueKeysWithValues: views.map { day in
			(day.date, day.allotments.map { CloseOutAllotment(story: $0.story, minutes: Int($0.minutes)) })
		})
	}
}

/// Receives the core's Monday alert during one check.
final class CloseOutAlertPort: NotifierPort, @unchecked Sendable {
	private let lock = NSLock()
	private var told: (weekStart: String, missing: [String])?

	func longTimer(sessionId _: String, elapsedSeconds _: UInt64) {}

	func thresholdCrossed(task _: String, story _: String?, reportedSeconds _: UInt64, estimateSeconds _: UInt64) {}

	func newSignals(workspace _: String, added _: UInt32, waiting _: UInt32) {}

	func closeOutMissing(workspace _: String, weekStart: String, missing: [String]) {
		lock.withLock { told = (weekStart, missing) }
	}

	func take() -> (weekStart: String, missing: [String])? {
		lock.withLock {
			defer { told = nil }
			return told
		}
	}
}

/// Which workspace's `Leave.md` counts for a workspace.
public enum LeaveSource {
	/// The root of the workspace named in `root`'s `leave_source`, else the
	/// first life workspace among `entries`; `nil` when there is none.
	public static func root(for root: String, among entries: [WorkspaceEntry]) -> String? {
		let candidates = entries.map { LeaveCandidateView(name: $0.name, isLife: $0.sphere == .life, root: $0.rootPath) }
		return try? resolveLeaveRoot(root: root, candidates: candidates) ?? nil
	}

	/// The week holding `date` with its working days, as a dry run.
	public static func workingWeek(of date: String, leaveRoot: String?) throws -> [CloseOutDay] {
		try VaultdayCore.workingWeek(date: date, leaveRoot: leaveRoot).days.map { CloseOutDay(date: $0.date, kind: CloseOutDayKind($0.kind)) }
	}
}

extension WorkspaceModel {
	/// The close-out of the open workspace, reading leave from `leaveRoot`;
	/// `nil` until the workspace and its app state are open.
	public func closeOutSource(leaveRoot: String?) -> (any CloseOutSource)? {
		guard let state, let vault, !workspaceId.isEmpty else { return nil }
		return CoreCloseOut(appState: session.appState, state: state, vault: vault, workspaceId: workspaceId, workspaceName: name, leaveRoot: leaveRoot)
	}
}
