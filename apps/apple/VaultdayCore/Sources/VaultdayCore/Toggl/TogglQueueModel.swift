import Foundation
import Observation

/// One entry of the Toggl queue, as Settings shows it.
public struct TogglQueueEntry: Identifiable, Hashable, Sendable {
	public enum State: Hashable, Sendable {
		case queued, sent, failed
	}

	public let key: String
	public let description: String
	public let story: String?
	public let durationSeconds: Int
	public let isBillable: Bool
	public let state: State
	public let attempts: Int
	public let lastError: String?

	public var id: String { key }

	public init(key: String, description: String, story: String?, durationSeconds: Int, isBillable: Bool, state: State, attempts: Int, lastError: String?) {
		self.key = key
		self.description = description
		self.story = story
		self.durationSeconds = durationSeconds
		self.isBillable = isBillable
		self.state = state
		self.attempts = attempts
		self.lastError = lastError
	}
}

/// What one drain did.
public struct TogglDrainOutcome: Equatable, Sendable {
	/// Why nothing was sent at all ("Toggl is not connected: …").
	public let notConnected: String?
	public let sent: Int
	public let retrying: Int
	public let failed: Int
	public let isUnauthorized: Bool

	public init(notConnected: String?, sent: Int, retrying: Int, failed: Int, isUnauthorized: Bool) {
		self.notConnected = notConnected
		self.sent = sent
		self.retrying = retrying
		self.failed = failed
		self.isUnauthorized = isUnauthorized
	}
}

/// Where the queue lives: the core in the app, a fake in tests. `drain`
/// blocks on the network once a live connection is allowed, so the model
/// calls it off the main actor.
public protocol TogglQueueSource: AnyObject, Sendable {
	func togglQueue() throws -> [TogglQueueEntry]
	func drainTogglQueue() throws -> TogglDrainOutcome
}

/// The Toggl queue of the open workspace and the scheduler that drains it:
/// on an interval and on demand. While the live connection is not allowed
/// (always, in this build) a drain answers why and sends nothing.
@MainActor
@Observable
public final class TogglQueueModel {
	public static let defaultInterval: Duration = .seconds(5 * 60)

	public private(set) var entries: [TogglQueueEntry] = []
	public private(set) var isDraining = false
	public private(set) var lastDrain: TogglDrainOutcome?
	public private(set) var errorMessage: String?

	@ObservationIgnored private var source: (any TogglQueueSource)?
	@ObservationIgnored private var loop: Task<Void, Never>?

	public init() {}

	public var queuedCount: Int { entries.count { $0.state == .queued } }
	public var failedCount: Int { entries.count { $0.state == .failed } }
	public var sentCount: Int { entries.count { $0.state == .sent } }

	/// "Toggl is not connected: allow_live is off", "2 queued · 1 failed", "All sent".
	public var statusText: String {
		if let reason = lastDrain?.notConnected { return reason }
		if lastDrain?.isUnauthorized == true { return "Toggl refused the token" }
		var parts: [String] = []
		if queuedCount > 0 { parts.append("\(queuedCount) queued") }
		if failedCount > 0 { parts.append("\(failedCount) failed") }
		if parts.isEmpty { return entries.isEmpty ? "Nothing queued" : "All sent" }
		return parts.joined(separator: " · ")
	}

	/// Points the model at a workspace's queue and reads it.
	public func load(source: (any TogglQueueSource)?) {
		self.source = source
		lastDrain = nil
		reload()
	}

	public func reload() {
		guard let source else {
			entries = []
			return
		}
		do {
			entries = try source.togglQueue()
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Drains now, then every `interval`, until ``stop()``.
	public func start(interval: Duration = TogglQueueModel.defaultInterval) {
		stop()
		loop = Task { [weak self] in
			while !Task.isCancelled {
				await self?.drainNow()
				try? await Task.sleep(for: interval)
			}
		}
	}

	public func stop() {
		loop?.cancel()
		loop = nil
	}

	/// One drain, unless one is under way.
	public func drainNow() async {
		guard !isDraining, let source else { return }
		isDraining = true
		let outcome: Result<TogglDrainOutcome, any Error> = await Task.detached {
			do {
				return .success(try source.drainTogglQueue())
			} catch {
				return .failure(error)
			}
		}.value
		isDraining = false
		switch outcome {
		case let .success(drained):
			lastDrain = drained
			errorMessage = nil
		case let .failure(error):
			errorMessage = coreErrorMessage(error)
		}
		reload()
	}
}
