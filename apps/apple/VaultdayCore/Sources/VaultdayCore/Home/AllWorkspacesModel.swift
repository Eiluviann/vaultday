import Foundation
import Observation

/// One workspace's part of the All view.
public struct WorkspaceGroup: Identifiable, Hashable, Sendable {
	public let id: String
	public let name: String
	public let accent: String?
	public let sphere: WorkspaceSphere
	/// The workspace's list for the day; ids carry the workspace so they stay unique.
	public let items: [PlanItem]
	public let plannedMinutes: Int
	/// Unfinished Pending Input tasks: work waiting on others.
	public let waitingCount: Int
	/// Why the workspace could not be read, when it could not.
	public let problem: String?
}

/// The All view: every workspace's list of one day side by side, read-only,
/// in the sidebar's order. Each workspace keeps its own order; nothing is
/// merged or summed across workspaces.
@MainActor
@Observable
public final class AllWorkspacesModel {
	/// The day shown, `YYYY-MM-DD`.
	public private(set) var date = ""
	public private(set) var groups: [WorkspaceGroup] = []

	/// Where tasks the stored order does not name yet join each list.
	public var newTaskPosition: ListPositionSetting = .bottom

	private let session: CoreSession?
	private var entries: [WorkspaceEntry] = []
	private weak var currentWorkspace: WorkspaceModel?
	private let now: () -> Date

	/// `session` opens the app state of workspaces other than the open one;
	/// without it their lists use the vault's order.
	public init(session: CoreSession?, now: @escaping () -> Date = Date.init) {
		self.session = session
		self.now = now
	}

	/// "Thursday 15 October".
	public var title: String {
		DayText.date(from: date).map(DayText.title) ?? date
	}

	public var isToday: Bool { date == DayText.string(now()) }

	/// Loads every workspace's list for `date` (`nil` keeps the day shown, or today).
	public func load(entries: [WorkspaceEntry], openWorkspace: WorkspaceModel?, date: String? = nil) {
		self.entries = entries
		currentWorkspace = openWorkspace
		self.date = date ?? (self.date.isEmpty ? DayText.string(now()) : self.date)
		groups = entries.map(group)
	}

	/// Shows the day `offset` days from the one shown.
	public func showDay(offset: Int) {
		guard let shown = DayText.date(from: date),
		      let target = Calendar.current.date(byAdding: .day, value: offset, to: shown)
		else { return }
		load(entries: entries, openWorkspace: currentWorkspace, date: DayText.string(target))
	}

	public func showToday() {
		load(entries: entries, openWorkspace: currentWorkspace, date: DayText.string(now()))
	}

	private func group(_ entry: WorkspaceEntry) -> WorkspaceGroup {
		do {
			let vault = try openWorkspace(path: entry.rootPath)
			let order = try state(for: entry)?.dayOrder(date: date) ?? []
			let list = try dayList(
				workspace: vault,
				date: date,
				storedOrder: order,
				placement: newTaskPosition == .top ? .top : .bottom,
				today: DayText.string(now()),
			)
			let items = list.items.map { PlanItem($0, idPrefix: "\(entry.id)/") }
			return WorkspaceGroup(
				id: entry.id,
				name: entry.name,
				accent: entry.accent,
				sphere: entry.sphere,
				items: items,
				plannedMinutes: Int(list.plannedMinutes),
				waitingCount: items.filter { $0.isPendingInput && !$0.isDone }.count,
				problem: nil,
			)
		} catch {
			return WorkspaceGroup(
				id: entry.id,
				name: entry.name,
				accent: entry.accent,
				sphere: entry.sphere,
				items: [],
				plannedMinutes: 0,
				waitingCount: 0,
				problem: coreErrorMessage(error),
			)
		}
	}

	/// The open workspace's state, else one opened for this load, so another
	/// workspace's order is as current as its change files; `nil` when its
	/// key has not reached this Mac, so the vault's order stands in.
	private func state(for entry: WorkspaceEntry) -> WorkspaceStateHandle? {
		if let currentWorkspace, currentWorkspace.workspaceId == entry.id, let state = currentWorkspace.state {
			return state
		}
		guard let session else { return nil }
		let environment = session.environment
		return try? openWorkspaceState(
			files: environment.files,
			workspaceRoot: entry.rootPath,
			workspaceId: entry.id,
			deviceId: session.deviceId,
			secrets: environment.secrets,
		)
	}
}
