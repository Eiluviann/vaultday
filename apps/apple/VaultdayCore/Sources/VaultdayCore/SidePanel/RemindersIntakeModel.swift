import Foundation
import Observation

/// A Reminders list a workspace takes items from, and the story they file into.
public struct IntakeList: Identifiable, Hashable, Sendable {
	public let listId: String
	public let title: String
	/// The story whose Next Steps get the items; `nil` files them into the intake note.
	public let story: String?

	public var id: String { listId }

	public init(listId: String, title: String, story: String?) {
		self.listId = listId
		self.title = title
		self.story = story
	}
}

/// What one intake run did.
public struct IntakeRun: Equatable, Sendable {
	/// Items filed as task lines and deleted from Reminders.
	public var filed = 0
	/// Items that stay in Reminders, with why: "Buy milk: alerts at a place".
	public var left: [String] = []
	/// The titles of the items that stay in Reminders, in the order met.
	public var leftTitles: [String] = []
	/// Items that could not be filed or deleted, with why; they are tried again next run.
	public var failed: [String] = []
}

/// The Reminders intake: every item of the workspace's intake lists becomes
/// a task line through the core's writer, then is deleted from Reminders.
/// The order is the safety rule: an item is deleted only after the core
/// confirms its line is written (or was written by an earlier, interrupted
/// run, which the line's `[reminder:: ]` field shows). No two-way mirror:
/// nothing flows back to Reminders.
@MainActor
@Observable
public final class RemindersIntakeModel {
	public private(set) var lists: [ReminderListInfo] = []
	public private(set) var lastRun: IntakeRun?
	public private(set) var message: String?
	public private(set) var isRunning = false

	private let store: any ReminderStoreAccess
	private let now: () -> Date

	public init(store: any ReminderStoreAccess, now: @escaping () -> Date = Date.init) {
		self.store = store
		self.now = now
	}

	public var hasAccess: Bool { store.access == .fullAccess }

	/// Asks for full access to Reminders, then reads the lists.
	public func requestAccess() async {
		do {
			guard try await store.requestFullAccess() else {
				message = Self.accessMessage
				return
			}
			message = nil
		} catch {
			message = error.localizedDescription
		}
		refreshLists()
	}

	/// Reads the lists the user can pick from.
	public func refreshLists() {
		lists = hasAccess ? store.lists() : []
	}

	/// Files every item of the open workspace's intake lists.
	@discardableResult
	public func run(workspace: WorkspaceModel) async -> IntakeRun? {
		guard hasAccess, !isRunning, let vault = workspace.vault, let state = workspace.state else { return nil }
		let intake = workspace.intakeLists
		guard !intake.isEmpty else { return nil }
		isRunning = true
		defer { isRunning = false }
		var run = IntakeRun()
		var items: [(item: ReminderItem, story: String?)] = []
		for list in intake {
			for item in await store.incompleteReminders(inList: list.listId) {
				items.append((item, list.story))
			}
		}
		file(items, vault: vault, state: state, into: &run)
		if run.filed > 0 {
			_ = try? state.flush()
		}
		lastRun = run
		message = Self.summary(of: run)
		return run
	}

	// MARK: Private

	static let accessMessage = "Vaultday needs full access to Reminders to file the items of your intake lists. Allow it in System Settings › Privacy & Security › Reminders."

	/// Files the whole batch in one core call, which reads the vault once;
	/// then deletes from Reminders each item whose line the core confirmed.
	private func file(_ items: [(item: ReminderItem, story: String?)], vault: WorkspaceHandle, state: WorkspaceStateHandle, into run: inout IntakeRun) {
		guard !items.isEmpty else { return }
		let outcomes: [ReminderImportOutcome]
		do {
			outcomes = try state.importReminders(
				workspace: vault,
				batch: items.map { ReminderImport(item: ReminderItemView($0.item), story: $0.story) },
				today: DayText.string(now()),
			)
		} catch {
			let message = coreErrorMessage(error)
			run.failed += items.map { "\($0.item.title): \(message)" }
			return
		}
		for (entry, outcome) in zip(items, outcomes) {
			record(outcome, for: entry.item, into: &run)
		}
	}

	private func record(_ outcome: ReminderImportOutcome, for item: ReminderItem, into run: inout IntakeRun) {
		switch outcome.intake {
		case .imported, .alreadyImported:
			do {
				try store.removeReminder(id: item.id)
				run.filed += 1
			} catch {
				run.failed.append("\(item.title): filed, but Reminders kept it (\(error.localizedDescription))")
			}
		case let .left(reason):
			run.left.append("\(item.title): \(reason)")
			run.leftTitles.append(item.title)
		case nil:
			run.failed.append("\(item.title): \(outcome.error ?? "not filed")")
		}
	}

	private static func summary(of run: IntakeRun) -> String? {
		var parts: [String] = []
		if run.filed > 0 { parts.append(run.filed == 1 ? "Filed 1 reminder" : "Filed \(run.filed) reminders") }
		if !run.leftTitles.isEmpty { parts.append("Left in Reminders: \(run.leftTitles.joined(separator: ", "))") }
		if !run.failed.isEmpty { parts.append("\(run.failed.count) not filed") }
		return parts.isEmpty ? nil : parts.joined(separator: " · ")
	}
}

extension ReminderItemView {
	init(_ item: ReminderItem) {
		self.init(
			externalId: item.externalId,
			title: item.title,
			dueDate: item.dueDate,
			hasDueTime: item.hasDueTime,
			isCompleted: item.isCompleted,
			isRecurring: item.isRecurring,
			hasLocationAlarm: item.hasLocationAlarm,
		)
	}
}
