import Foundation
import Observation

/// A story the confirmation's Adjust can route a task to.
public struct StoryChoice: Identifiable, Hashable, Sendable {
	public var id: String { number }
	public let number: String
	public let title: String
}

/// What one quick-add line was understood as: the confirmation popover.
/// Every field can be changed with Adjust before Save writes anything.
public struct QuickAddEntry: Equatable, Sendable {
	public enum Kind: Equatable, Sendable {
		case task
		case event
	}

	public var kind: Kind
	public var text: String
	public var storyNumber: String?
	public var storyTitle: String?
	/// Whether the story is one of the workspace's; an unknown ticket number cannot be saved to.
	public var isStoryKnown: Bool
	/// `YYYY-MM-DD`.
	public var day: String
	/// `HH:MM`, events only.
	public var startTime: String?
	public var endTime: String?
	public var estimateMinutes: Int?
	public var isPendingInput: Bool
	public var waitingOn: String?
	/// The id the task gets when saved, shown so it can be quoted at once.
	public let id: String
	public let warnings: [String]
	/// The workspace a `/name` routed the line to, when it is not the open one.
	public var routedWorkspaceName: String?

	/// The kind as the popover's head names it.
	public var kindTitle: String { kind == .task ? "Task" : "Event" }

	/// The switch beside the kind: "switch to Event" or "switch to Task".
	public var switchTitle: String { kind == .task ? "switch to Event" : "switch to Task" }

	/// The chips under the text, in the board's order: story, day, estimate (or time), section.
	public var chips: [String] {
		var chips: [String] = []
		if let storyNumber { chips.append(storyNumber) }
		chips.append(dayChip)
		if kind == .event, let startTime {
			chips.append(endTime.map { "\(startTime) – \($0)" } ?? startTime)
		}
		if let estimateMinutes { chips.append(DurationText.minutes(estimateMinutes)) }
		if kind == .task { chips.append(sectionChip) }
		return chips
	}

	/// "Fri 16 Oct".
	public var dayChip: String {
		DayText.date(from: day).map(DayText.chip) ?? day
	}

	/// "for you · Next Steps" or "for Dana · Pending Input".
	public var sectionChip: String {
		isPendingInput ? "for \(waitingOn ?? "someone") · Pending Input" : "for you · Next Steps"
	}

	/// Why Save is not offered yet, `nil` when it is.
	public var saveBlocker: String? {
		if let routedWorkspaceName { return "Routed to \(routedWorkspaceName); add it from that workspace" }
		if kind == .event { return "Events reach the calendar in a later version" }
		if text.trimmingCharacters(in: .whitespaces).isEmpty { return "Type what the task is" }
		guard storyNumber != nil else { return "Pick a story with Adjust" }
		if !isStoryKnown { return "\(storyNumber ?? "") is not a story here; pick one with Adjust" }
		return nil
	}

	public var canSave: Bool { saveBlocker == nil }
}

/// Quick add: one field, then a confirmation of what the core's quick-add
/// grammar understood. Nothing is written until Save; Esc cancels.
@MainActor
@Observable
public final class QuickAddModel {
	/// The field's text.
	public var text = ""
	/// The confirmation, while it is open.
	public private(set) var entry: QuickAddEntry?
	/// Whether Adjust turned the chips into fields.
	public private(set) var isAdjusting = false
	/// The stories Adjust offers.
	public private(set) var stories: [StoryChoice] = []
	public private(set) var message: String?

	private var vault: WorkspaceHandle?
	private var state: WorkspaceStateHandle?
	/// The workspaces `/name` may route to.
	private var workspaces: [QuickAddWorkspaceView] = []
	/// The open workspace's id; a line routed elsewhere is not saved here.
	private var currentWorkspaceId: String?
	/// Adjust's estimate field holds text the grammar cannot read; Save waits for it.
	private var hasUnreadableEstimate = false
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	/// Points quick add at the open workspace; `workspaces` are the ones `/name` may name.
	public func attach(to workspace: WorkspaceModel, workspaces: [WorkspaceEntry] = []) {
		vault = workspace.vault
		state = workspace.state
		currentWorkspaceId = workspace.workspaceId
		self.workspaces = workspaces.map { QuickAddWorkspaceView(id: $0.id, name: $0.name) }
		cancel()
	}

	/// Opens quick add on the workspace at `path` without app state (tests).
	public func attach(workspacePath path: String, workspaces: [WorkspaceEntry] = []) throws {
		vault = try openWorkspace(path: path)
		state = nil
		currentWorkspaceId = nil
		self.workspaces = workspaces.map { QuickAddWorkspaceView(id: $0.id, name: $0.name) }
		cancel()
	}

	/// Add or ⏎: reads the line and opens the confirmation. Writes nothing.
	public func submit() {
		guard let vault else { return }
		let line = text.trimmingCharacters(in: .whitespaces)
		guard !line.isEmpty else { return }
		do {
			let view = try parseQuickAdd(
				workspace: vault,
				line: line,
				nextId: try nextId(vault),
				workspaces: workspaces,
				today: DayText.string(now()),
			)
			var entry = QuickAddEntry(view)
			if let routed = view.workspace, routed != currentWorkspaceId {
				entry.routedWorkspaceName = workspaces.first { $0.id == routed }?.name ?? routed
			}
			self.entry = entry
			isAdjusting = false
			message = nil
		} catch {
			message = coreErrorMessage(error)
		}
	}

	/// "switch to Event" / "switch to Task".
	public func switchKind() {
		guard let kind = entry?.kind else { return }
		entry?.kind = kind == .task ? .event : .task
	}

	/// Adjust: the chips become fields, and the workspace's stories are offered.
	public func adjust() {
		guard let vault else { return }
		isAdjusting = true
		stories = ((try? storyChoices(workspace: vault)) ?? []).map { StoryChoice(number: $0.number, title: $0.title) }
	}

	/// Routes the entry to one of the workspace's stories.
	public func setStory(_ story: StoryChoice?) {
		entry?.storyNumber = story?.number
		entry?.storyTitle = story?.title
		entry?.isStoryKnown = story != nil
	}

	public func setText(_ text: String) {
		entry?.text = text
	}

	/// Sets the day the task is planned for.
	public func setDay(_ day: Date) {
		entry?.day = DayText.string(day)
	}

	/// Sets the estimate from what was typed (`45m`, `1h 30m`, `1.5h`); empty clears it.
	@discardableResult
	public func setEstimate(_ text: String) -> Bool {
		let trimmed = text.replacingOccurrences(of: " ", with: "")
		guard !trimmed.isEmpty else {
			entry?.estimateMinutes = nil
			hasUnreadableEstimate = false
			return true
		}
		guard let minutes = QuickAddModel.minutes(in: trimmed) else {
			hasUnreadableEstimate = true
			message = Self.unreadableEstimateMessage
			return false
		}
		entry?.estimateMinutes = minutes
		hasUnreadableEstimate = false
		message = nil
		return true
	}

	public func setPendingInput(_ isPendingInput: Bool, waitingOn: String?) {
		entry?.isPendingInput = isPendingInput
		let name = waitingOn?.trimmingCharacters(in: .whitespaces)
		entry?.waitingOn = isPendingInput && name?.isEmpty == false ? name : nil
	}

	/// Save ⏎: writes one line under the story's section and closes. `true` when written.
	@discardableResult
	public func save() -> Bool {
		guard let vault, let entry else { return false }
		if hasUnreadableEstimate {
			message = Self.unreadableEstimateMessage
			return false
		}
		if let blocker = entry.saveBlocker {
			message = blocker
			return false
		}
		do {
			_ = try saveQuickAdd(
				workspace: vault,
				input: QuickAddSaveInput(
					id: entry.id,
					text: entry.text,
					storyNumber: entry.storyNumber,
					isPendingInput: entry.isPendingInput,
					waitingOn: entry.waitingOn,
					day: entry.day,
					estimateMinutes: entry.estimateMinutes.map(UInt32.init),
				),
				created: DayText.string(now()),
			)
			text = ""
			cancel()
			return true
		} catch {
			message = coreErrorMessage(error)
			return false
		}
	}

	/// Esc: closes the confirmation; the field keeps its text.
	public func cancel() {
		entry = nil
		isAdjusting = false
		hasUnreadableEstimate = false
		message = nil
	}

	/// The id the entry shows: minted from this Mac's range when the app
	/// state is open, so two Macs never hand out the same one.
	private func nextId(_ vault: WorkspaceHandle) throws -> String {
		if let state {
			return try state.mintTaskId(workspace: vault)
		}
		return try nextTaskId(workspace: vault)
	}

	private static let unreadableEstimateMessage = "Use an estimate like 45m or 1h 30m, up to 24h"

	/// `45m`, `2h`, `1h30m`, `1.5h` in minutes, up to a day, read by the
	/// core's quick-add grammar so Adjust accepts what the line does.
	static func minutes(in text: String) -> Int? {
		parseEstimateMinutes(text: text).map(Int.init)
	}
}

extension QuickAddEntry {
	init(_ view: QuickAddView) {
		self.init(
			kind: view.kind == .task ? .task : .event,
			text: view.text,
			storyNumber: view.storyNumber,
			storyTitle: view.storyTitle,
			isStoryKnown: view.isStoryKnown,
			day: view.day,
			startTime: view.startTime,
			endTime: view.endTime,
			estimateMinutes: view.estimateMinutes.map(Int.init),
			isPendingInput: view.isPendingInput,
			waitingOn: view.waitingOn,
			id: view.id,
			warnings: view.warnings,
			routedWorkspaceName: nil,
		)
	}
}
