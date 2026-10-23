import Foundation
import Observation

/// The AI level step: how much AI does, everywhere, per sphere or per
/// workspace, and what a cloud provider may keep. The base version has no AI,
/// so every choice is stored through the core and inert, and says Planned.
@MainActor
@Observable
public final class AiLevelStepModel {
	/// The name the level's scope is stored under.
	static let scopeName = "level"
	public static let plannedNote = "AI arrives in a later release; the level you pick is remembered for then"

	public private(set) var scope = AiScope()
	public private(set) var retention: DataRetentionSetting = .zeroRetention
	public private(set) var errorMessage: String?

	private let session: CoreSession
	private var workspaces: [WorkspaceEntry] = []

	public init(session: CoreSession) {
		self.session = session
		reload()
	}

	public func reload() {
		scope = AiScope(session.appState.aiScope(name: Self.scopeName))
		retention = DataRetentionSetting(session.appState.dataRetention())
	}

	/// Tells the model which workspaces exist.
	public func refresh(workspaces: [WorkspaceEntry]) {
		self.workspaces = workspaces
	}

	/// The level a sphere, a workspace or (for ``AiScope/wholeScope``) everything has.
	public func level(of target: String) -> AiLevelChoice {
		switch scope.mode {
		case .off: .off
		case .everywhere:
			target == AiScope.wholeScope ? AiLevelChoice(scope.wholeTier) : .off
		case .perSphere, .perWorkspace:
			scope.targets.contains(target) ? AiLevelChoice(scope.tiers[target] ?? .glance) : .off
		}
	}

	/// Sets where the level applies. Everything starts at Glance, the light level.
	public func setMode(_ mode: FeatureScopeSwitch) {
		var next = AiScope(mode: mode, targets: defaultTargets(for: mode, workspaces: workspaces))
		switch mode {
		case .off: break
		case .everywhere: next.tiers = [AiScope.wholeScope: .glance]
		case .perSphere, .perWorkspace: next.tiers = Dictionary(uniqueKeysWithValues: next.targets.map { ($0, AiTier.glance) })
		}
		store(next)
	}

	/// Sets the level of one sphere or workspace, or of everything. Off drops the target; Off for everything switches the level off.
	public func setLevel(_ choice: AiLevelChoice, target: String) {
		var next = scope
		switch (scope.mode, choice) {
		case (.off, _):
			return
		case (.everywhere, .off):
			next = AiScope()
		case (.everywhere, .glance), (.everywhere, .insight):
			next.tiers = [AiScope.wholeScope: choice == .glance ? .glance : .insight]
		case (_, .off):
			next.targets.remove(target)
			next.tiers[target] = nil
		case (_, .glance), (_, .insight):
			next.targets.insert(target)
			next.tiers[target] = choice == .glance ? .glance : .insight
		}
		store(next)
	}

	public func setRetention(_ retention: DataRetentionSetting) {
		do {
			try session.edit { try $0.setDataRetention(retention: retention.core) }
			self.retention = retention
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	private func store(_ next: AiScope) {
		do {
			try session.edit { try $0.setAiScope(name: Self.scopeName, scope: next.core) }
			scope = next
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}
}

extension AiLevelChoice {
	init(_ tier: AiTier) {
		self = tier == .glance ? .glance : .insight
	}
}
