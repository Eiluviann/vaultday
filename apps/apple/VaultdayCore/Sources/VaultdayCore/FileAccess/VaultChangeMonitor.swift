import Foundation

/// Watches the open workspace's folder through the core's file watcher
/// (PR 5) and tells the app, on the main actor, once a burst of changes
/// made outside the app has settled. The core debounces raw events
/// (``coreDebounceMilliseconds``); this side coalesces the batches that still arrive
/// close together (an editor autosaving while you type) into one call
/// after ``quietPeriod`` without a batch, so a surface reads the vault once.
@MainActor
public final class VaultChangeMonitor {
	/// The core watcher's debounce.
	public static let coreDebounceMilliseconds: UInt32 = 300
	/// How long no batch must arrive before ``onChange`` runs.
	public let quietPeriod: Duration
	/// Called with the paths changed since the last call, root-relative.
	public var onChange: ([String]) -> Void = { _ in }

	public private(set) var isWatching = false
	/// Which watch a batch belongs to; a batch of a stopped watch is dropped.
	private(set) var currentGeneration = 0
	private var watch: VaultChangeWatch?
	private var pending: Set<String> = []
	private var flush: Task<Void, Never>?

	public init(quietPeriod: Duration = .seconds(1)) {
		self.quietPeriod = quietPeriod
	}

	/// Watches `workspace`; a watch already running is stopped first.
	/// `false`, with nothing watched, when the folder cannot be watched.
	@discardableResult
	public func start(_ workspace: WorkspaceModel) -> Bool {
		stop()
		guard let vault = workspace.vault else { return false }
		currentGeneration += 1
		let listener = Listener(monitor: self, generation: currentGeneration)
		do {
			watch = try VaultChangeWatch.start(
				workspace: vault,
				debounceMs: Self.coreDebounceMilliseconds,
				listener: listener,
			)
			isWatching = true
			return true
		} catch {
			return false
		}
	}

	/// Stops the watch and drops the batches not yet delivered.
	public func stop() {
		watch?.stop()
		watch = nil
		flush?.cancel()
		flush = nil
		pending = []
		isWatching = false
		currentGeneration += 1
	}

	/// A batch from the core: kept, and the quiet period starts again.
	func receive(_ paths: [String], generation batchGeneration: Int) {
		guard batchGeneration == currentGeneration, isWatching else { return }
		pending.formUnion(paths)
		flush?.cancel()
		let quietPeriod = quietPeriod
		flush = Task { [weak self] in
			try? await Task.sleep(for: quietPeriod)
			guard !Task.isCancelled else { return }
			self?.deliver()
		}
	}

	private func deliver() {
		guard !pending.isEmpty else { return }
		let paths = pending.sorted()
		pending = []
		onChange(paths)
	}

	/// The core's listener: called on the watch thread, it hops to the main actor.
	private final class Listener: VaultChangeListener, @unchecked Sendable {
		// Read only on the main actor.
		private weak var monitor: VaultChangeMonitor?
		private let generation: Int

		init(monitor: VaultChangeMonitor, generation: Int) {
			self.monitor = monitor
			self.generation = generation
		}

		func vaultChanged(paths: [String]) {
			let generation = generation
			Task { @MainActor [weak self] in
				self?.monitor?.receive(paths, generation: generation)
			}
		}
	}
}
