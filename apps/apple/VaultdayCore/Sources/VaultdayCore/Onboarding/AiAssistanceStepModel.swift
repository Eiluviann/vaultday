import Foundation
import Observation

/// What an AI feature can be set to: either level, or only one.
public enum AiFeatureTiers: Sendable {
	case either
	case glanceOnly
	case insightOnly
}

/// An AI feature the assistance step lists.
public struct AiFeature: Identifiable, Hashable, Sendable {
	public let id: String
	public let title: String
	public let description: String
	public let tiers: AiFeatureTiers
	/// The line under a feature that works at both levels, saying what Insight adds.
	public let insightNote: String?

	public static let all: [AiFeature] = [
		AiFeature(id: "sort-tasks", title: "Sort tasks", description: "Type and priority from the task", tiers: .either, insightNote: "Insight adds its project and related notes"),
		AiFeature(id: "sort-inbox", title: "Sort the inbox", description: "An action for each mail and message", tiers: .either, insightNote: "Insight links it to the related task"),
		AiFeature(id: "suggestions", title: "Suggestions", description: "Next steps from the item", tiers: .either, insightNote: "Insight draws on notes and history"),
		AiFeature(id: "search", title: "Search", description: "AI-assisted search", tiers: .either, insightNote: "Insight searches everything by meaning"),
		AiFeature(id: "quick-add", title: "Quick add in plain words", description: "“Dentist Fri 9” becomes a timed reminder", tiers: .glanceOnly, insightNote: nil),
		AiFeature(id: "estimates", title: "Estimates", description: "How long a task should take", tiers: .glanceOnly, insightNote: nil),
		AiFeature(id: "plan-the-day", title: "Plan the day", description: "Fits tasks around the calendar in morning planning", tiers: .insightOnly, insightNote: nil),
		AiFeature(id: "summaries", title: "Summaries", description: "Long threads, meeting notes and pages", tiers: .insightOnly, insightNote: nil),
		AiFeature(id: "drafts", title: "Drafts", description: "Replies and status updates, never sent by AI", tiers: .insightOnly, insightNote: nil),
		AiFeature(id: "day-log", title: "Day log and weekly review", description: "A first draft of the log and the week's close-out", tiers: .insightOnly, insightNote: nil),
	]
}

/// What other AI apps may do through the `vaultday` command.
public enum OtherAppsAccessSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case off
	case readOnly
	case readAndWrite

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .off: "Off"
		case .readOnly: "Read only"
		case .readAndWrite: "Read and write"
		}
	}

	var core: OtherAppsAccessView {
		switch self {
		case .off: .off
		case .readOnly: .readOnly
		case .readAndWrite: .readAndWrite
		}
	}

	init(_ view: OtherAppsAccessView) {
		switch view {
		case .off: self = .off
		case .readOnly: self = .readOnly
		case .readAndWrite: self = .readAndWrite
		}
	}
}

/// What an agent may change without asking.
public enum AgentChangesSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case askEveryTime
	case askBeforeDeleting
	case neverAsk

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .askEveryTime: "Ask every time"
		case .askBeforeDeleting: "Ask before deleting"
		case .neverAsk: "Never ask"
		}
	}

	var core: AgentChangesView {
		switch self {
		case .askEveryTime: .askEveryTime
		case .askBeforeDeleting: .askBeforeDeleting
		case .neverAsk: .neverAsk
		}
	}

	init(_ view: AgentChangesView) {
		switch view {
		case .askEveryTime: self = .askEveryTime
		case .askBeforeDeleting: self = .askBeforeDeleting
		case .neverAsk: self = .neverAsk
		}
	}
}

/// The AI assistance step: which features are on and where, at which level,
/// what agents and other AI apps may do, and the guardrails. Stored through
/// the core and inert in the base version, which has no AI.
@MainActor
@Observable
public final class AiAssistanceStepModel {
	/// The name Claude Code's scope is stored under.
	static let claudeCodeName = "claude-code"
	public static let plannedNote = "AI arrives in a later release; these choices are remembered for then"

	public private(set) var scopes: [String: AiScope] = [:]
	public private(set) var claudeCode = AiScope()
	public private(set) var otherApps: OtherAppsAccessSetting = .off
	public private(set) var agentChanges: AgentChangesSetting = .askEveryTime
	public private(set) var previewsBeforeSending = true
	public private(set) var keepsActivityLog = true
	public private(set) var errorMessage: String?

	private let session: CoreSession
	private var workspaces: [WorkspaceEntry] = []

	public init(session: CoreSession) {
		self.session = session
		reload()
	}

	public func reload() {
		let appState = session.appState
		scopes = Dictionary(uniqueKeysWithValues: AiFeature.all.map { ($0.id, AiScope(appState.aiScope(name: $0.id))) })
		claudeCode = AiScope(appState.aiScope(name: Self.claudeCodeName))
		otherApps = OtherAppsAccessSetting(appState.otherAppsAccess())
		agentChanges = AgentChangesSetting(appState.agentChanges())
		previewsBeforeSending = appState.previewsBeforeSending()
		keepsActivityLog = appState.keepsAiActivityLog()
	}

	/// Tells the model which workspaces exist.
	public func refresh(workspaces: [WorkspaceEntry]) {
		self.workspaces = workspaces
	}

	public func scope(of feature: String) -> AiScope { scopes[feature] ?? AiScope() }

	/// Sets where a feature is on. A feature that works at both levels starts at Glance.
	public func setMode(_ mode: FeatureScopeSwitch, feature: String) {
		guard let descriptor = AiFeature.all.first(where: { $0.id == feature }) else { return }
		var next = scope(of: feature)
		next.mode = mode
		next.targets = defaultTargets(for: mode, workspaces: workspaces)
		if mode == .off {
			next.tiers = [:]
		} else if descriptor.tiers == .either, next.tiers[AiScope.wholeScope] == nil {
			next.tiers = [AiScope.wholeScope: .glance]
		}
		store(next, name: feature)
	}

	/// Ticks or unticks a sphere or a workspace in a feature's scope row.
	public func setTarget(_ target: String, isOn: Bool, feature: String) {
		var next = scope(of: feature)
		if isOn { next.targets.insert(target) } else { next.targets.remove(target) }
		store(next, name: feature)
	}

	/// Sets the level of a feature that works at both.
	public func setTier(_ tier: AiTier, feature: String) {
		guard AiFeature.all.first(where: { $0.id == feature })?.tiers == .either else { return }
		var next = scope(of: feature)
		next.tiers[AiScope.wholeScope] = tier
		store(next, name: feature)
	}

	public func setClaudeCodeMode(_ mode: FeatureScopeSwitch) {
		var next = AiScope(mode: mode, targets: defaultTargets(for: mode, workspaces: workspaces))
		if mode == .off { next.tiers = [:] }
		store(next, name: Self.claudeCodeName)
	}

	public func setClaudeCodeTarget(_ target: String, isOn: Bool) {
		var next = claudeCode
		if isOn { next.targets.insert(target) } else { next.targets.remove(target) }
		store(next, name: Self.claudeCodeName)
	}

	public func setOtherApps(_ access: OtherAppsAccessSetting) {
		write { try $0.setOtherAppsAccess(access: access.core) }
	}

	public func setAgentChanges(_ changes: AgentChangesSetting) {
		write { try $0.setAgentChanges(changes: changes.core) }
	}

	public func setPreviewsBeforeSending(_ isOn: Bool) {
		write { try $0.setPreviewsBeforeSending(isOn: isOn) }
	}

	public func setKeepsActivityLog(_ isOn: Bool) {
		write { try $0.setKeepsAiActivityLog(isOn: isOn) }
	}

	private func store(_ next: AiScope, name: String) {
		write { try $0.setAiScope(name: name, scope: next.core) }
	}

	private func write(_ edit: (AppStateHandle) throws -> Void) {
		do {
			try session.edit(edit)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reload()
	}
}
