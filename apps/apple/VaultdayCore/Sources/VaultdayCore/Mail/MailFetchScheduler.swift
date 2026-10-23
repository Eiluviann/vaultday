import AppKit
import Foundation
import Observation

/// What the last mail fetch did, for the Inbox column.
public struct MailFetchSummary: Equatable, Sendable {
	public let finishedAt: Date
	public let added: Int
	/// One line per account that failed, in words for the user.
	public let failures: [String]
}

/// Fetches the open workspace's mail accounts into its signal store on an
/// interval (from Settings, five minutes until changed) and on demand. The
/// core's fetch blocks on the network, so it runs on a detached task; the
/// main actor only starts it and reads the summary.
@MainActor
@Observable
public final class MailFetchScheduler {
	public private(set) var isFetching = false
	public private(set) var lastFetch: MailFetchSummary?
	/// Signals that are new right now in the open workspace.
	public private(set) var newSignalCount = 0
	/// Called on the main actor after every fetch, so the Inbox reads the store again.
	@ObservationIgnored public var onFetched: () -> Void = {}
	/// The quiet notification: called once for a fetch that brought new
	/// signals, never per signal, with how many came and how many are new now.
	@ObservationIgnored public var onNewSignals: (_ added: Int, _ waiting: Int) -> Void = { _, _ in }

	private let session: CoreSession
	@ObservationIgnored private var loop: Task<Void, Never>?
	@ObservationIgnored private weak var workspace: WorkspaceModel?
	/// The workspace and interval the running schedule was started for.
	@ObservationIgnored private var scheduledKey: String?

	public init(session: CoreSession) {
		self.session = session
	}

	/// Fetches now and then every interval for `workspace`. A schedule
	/// already running for the same workspace and interval is kept unless
	/// `isForced` (an account was added or removed).
	public func start(for workspace: WorkspaceModel, isForced: Bool = false) {
		let seconds = max(session.appState.mailFetchIntervalSeconds(), 60)
		let key = "\(workspace.workspaceId)|\(seconds)"
		if !isForced, loop != nil, scheduledKey == key { return }
		stop()
		self.workspace = workspace
		scheduledKey = key
		loop = Task { [weak self] in
			while !Task.isCancelled {
				await self?.fetchNow()
				try? await Task.sleep(for: .seconds(Int(seconds)))
			}
		}
	}

	/// Stops the schedule; a fetch under way finishes and is recorded.
	public func stop() {
		loop?.cancel()
		loop = nil
		scheduledKey = nil
	}

	/// Fetches the open workspace's accounts once, unless a fetch is under way.
	public func fetchNow() async {
		guard !isFetching, let workspace, let state = workspace.state, !workspace.workspaceId.isEmpty else { return }
		let workspaceId = workspace.workspaceId
		let hasAccounts = session.appState.mailAccounts().contains { $0.workspace == workspaceId && !$0.folders.isEmpty }
		guard hasAccounts else {
			newSignalCount = 0
			return
		}
		isFetching = true
		let appState = session.appState
		let outcome: Result<MailFetchView, any Error> = await Task.detached {
			do {
				let fetched = try appState.fetchMail(state: state, workspace: workspaceId, connection: nil)
				_ = try state.flush()
				return .success(fetched)
			} catch {
				return .failure(error)
			}
		}.value
		isFetching = false
		switch outcome {
		case let .success(fetched):
			lastFetch = MailFetchSummary(
				finishedAt: Date(),
				added: Int(fetched.added),
				failures: fetched.failures.map { "\($0.connection): \($0.message)" },
			)
			// The core decides whether this fetch is told; the port only carries it here.
			let port = SignalAnnouncementPort()
			if state.announceNewSignals(workspace: workspaceId, added: fetched.added, notifier: port),
			   let announced = port.announced
			{
				onNewSignals(announced.added, announced.waiting)
			}
		case let .failure(error):
			lastFetch = MailFetchSummary(finishedAt: Date(), added: 0, failures: [coreErrorMessage(error)])
		}
		newSignalCount = Int(state.newSignalCount())
		onFetched()
	}
}

/// The core's `Notifier` port for one fetch: it keeps the announcement so
/// the notification is posted on the main actor after the call returns.
final class SignalAnnouncementPort: NotifierPort, @unchecked Sendable {
	// Only touched during the synchronous `announceNewSignals` call and right after it.
	private let lock = NSLock()
	private var stored: (added: Int, waiting: Int)?

	var announced: (added: Int, waiting: Int)? {
		lock.withLock { stored }
	}

	func longTimer(sessionId _: String, elapsedSeconds _: UInt64) {}

	func thresholdCrossed(task _: String, story _: String?, reportedSeconds _: UInt64, estimateSeconds _: UInt64) {}

	func newSignals(workspace _: String, added: UInt32, waiting: UInt32) {
		lock.withLock { stored = (Int(added), Int(waiting)) }
	}

	func closeOutMissing(workspace _: String, weekStart _: String, missing _: [String]) {}
}

/// Opens a signal's message in its account's mail app: the core hands back
/// only `message:` and `https:` links (PR 13 binds this to the signal cards).
@MainActor
public enum SignalLinks {
	/// Opens the message behind `signalId`; returns whether a link was opened.
	@discardableResult
	public static func open(signalId: String, in workspace: WorkspaceModel) -> Bool {
		guard let state = workspace.state,
		      let link = try? state.signalLink(id: signalId),
		      let url = URL(string: link)
		else { return false }
		return NSWorkspace.shared.open(url)
	}
}
