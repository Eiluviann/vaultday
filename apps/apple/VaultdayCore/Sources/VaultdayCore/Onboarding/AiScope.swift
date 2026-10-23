import Foundation

/// How much an AI feature may read.
public enum AiTier: String, CaseIterable, Identifiable, Hashable, Sendable {
	case glance
	case insight

	public var id: String { rawValue }

	public var title: String { self == .glance ? "Glance" : "Insight" }

	var core: AiTierView { self == .glance ? .glance : .insight }

	init(_ view: AiTierView) {
		self = view == .glance ? .glance : .insight
	}
}

/// The level an AI level row shows for a sphere or workspace.
public enum AiLevelChoice: String, CaseIterable, Identifiable, Hashable, Sendable {
	case off
	case glance
	case insight

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .off: "Off"
		case .glance: "AI Glance"
		case .insight: "AI Insight"
		}
	}
}

/// Where an AI level or feature is on, and the tier for each target.
public struct AiScope: Equatable, Sendable {
	public var mode: FeatureScopeSwitch = .off
	/// Sphere names for ``FeatureScopeSwitch/perSphere``, workspace ids for ``FeatureScopeSwitch/perWorkspace``.
	public var targets: Set<String> = []
	/// The tier by target; ``AiScope/wholeScope`` is the tier of the scope as one.
	public var tiers: [String: AiTier] = [:]

	/// The tier key that applies to everything the scope covers.
	public static let wholeScope = "*"

	public init(mode: FeatureScopeSwitch = .off, targets: Set<String> = [], tiers: [String: AiTier] = [:]) {
		self.mode = mode
		self.targets = targets
		self.tiers = tiers
	}

	/// The tier of the scope as one, Glance until chosen.
	public var wholeTier: AiTier { tiers[Self.wholeScope] ?? .glance }

	init(_ view: AiScopeView) {
		self.init(
			mode: FeatureScopeSwitch(view.mode),
			targets: Set(view.targets),
			tiers: Dictionary(view.tiers.map { ($0.target, AiTier($0.tier)) }, uniquingKeysWith: { _, last in last }),
		)
	}

	var core: AiScopeView {
		AiScopeView(
			mode: mode.core,
			targets: targets.sorted(),
			tiers: tiers.sorted { $0.key < $1.key }.map { AiTierEntryView(target: $0.key, tier: $0.value.core) },
		)
	}
}

/// What a cloud provider may keep of the requests.
public enum DataRetentionSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case disabled
	case safetyRetention
	case zeroRetention

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .disabled: "Disabled"
		case .safetyRetention: "Safety retention allowed"
		case .zeroRetention: "Zero data retention"
		}
	}

	var core: DataRetentionView {
		switch self {
		case .disabled: .disabled
		case .safetyRetention: .safetyRetention
		case .zeroRetention: .zeroRetention
		}
	}

	init(_ view: DataRetentionView) {
		switch view {
		case .disabled: self = .disabled
		case .safetyRetention: self = .safetyRetention
		case .zeroRetention: self = .zeroRetention
		}
	}
}

/// The targets a scope switch starts with: every sphere, or every workspace.
func defaultTargets(for mode: FeatureScopeSwitch, workspaces: [WorkspaceEntry]) -> Set<String> {
	switch mode {
	case .off, .everywhere: []
	case .perSphere: Set(WorkspaceSphere.allCases.map(\.rawValue))
	case .perWorkspace: Set(workspaces.map(\.id))
	}
}
