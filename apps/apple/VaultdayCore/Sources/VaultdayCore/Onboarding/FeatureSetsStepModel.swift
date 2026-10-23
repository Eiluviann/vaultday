import Foundation
import Observation

/// The four-way switch of a feature tile.
public enum FeatureScopeSwitch: String, CaseIterable, Identifiable, Hashable, Sendable {
	case off
	case everywhere
	case perSphere
	case perWorkspace

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .off: "Off"
		case .everywhere: "Everywhere"
		case .perSphere: "Per sphere"
		case .perWorkspace: "Per workspace"
		}
	}

	var core: FeatureScopeModeView {
		switch self {
		case .off: .off
		case .everywhere: .everywhere
		case .perSphere: .perSphere
		case .perWorkspace: .perWorkspace
		}
	}

	init(_ view: FeatureScopeModeView) {
		switch view {
		case .off: self = .off
		case .everywhere: self = .everywhere
		case .perSphere: self = .perSphere
		case .perWorkspace: self = .perWorkspace
		}
	}
}

/// A planned feature set as a tile.
public struct FeatureTile: Identifiable, Hashable, Sendable {
	public let id: String
	public let title: String
	public let description: String
	/// Whether switching it on will send data to a new third party.
	public let raisesEgress: Bool
	public var scope: FeatureScopeSwitch
	/// Sphere names for ``FeatureScopeSwitch/perSphere``, workspace ids for ``FeatureScopeSwitch/perWorkspace``.
	public var targets: Set<String>
}

/// The Feature sets step. No feature set ships in the base version, so every
/// tile is inert and tagged Planned; the switch is stored through the core's
/// feature registry (the app switch and one per workspace) for when one does.
@MainActor
@Observable
public final class FeatureSetsStepModel {
	public private(set) var tiles: [FeatureTile] = []
	public private(set) var errorMessage: String?

	private let session: CoreSession
	private var workspaces: [WorkspaceEntry] = []

	public init(session: CoreSession) {
		self.session = session
		tiles = plannedFeatureSets().map { planned in
			let stored = session.appState.featureScope(feature: planned.id)
			return FeatureTile(
				id: planned.id,
				title: planned.title,
				description: planned.description,
				raisesEgress: planned.raisesEgress,
				scope: FeatureScopeSwitch(stored.mode),
				targets: Set(stored.targets),
			)
		}
	}

	/// Tells the model which workspaces exist and writes every tile's
	/// registry switches again, so a workspace added later is covered.
	public func refresh(workspaces: [WorkspaceEntry]) {
		self.workspaces = workspaces
		for tile in tiles where tile.scope != .off {
			store(tile)
		}
	}

	/// Sets a tile's switch. Per sphere and per workspace start with every
	/// sphere or workspace ticked.
	public func setScope(_ scope: FeatureScopeSwitch, tile id: String) {
		guard let index = tiles.firstIndex(where: { $0.id == id }) else { return }
		var tile = tiles[index]
		tile.scope = scope
		switch scope {
		case .off, .everywhere: tile.targets = []
		case .perSphere: tile.targets = Set(WorkspaceSphere.allCases.map(\.rawValue))
		case .perWorkspace: tile.targets = Set(workspaces.map(\.id))
		}
		tiles[index] = tile
		store(tile)
	}

	/// Ticks or unticks a sphere or a workspace in a tile's scope row.
	public func setTarget(_ target: String, isOn: Bool, tile id: String) {
		guard let index = tiles.firstIndex(where: { $0.id == id }) else { return }
		var tile = tiles[index]
		if isOn { tile.targets.insert(target) } else { tile.targets.remove(target) }
		tiles[index] = tile
		store(tile)
	}

	private func store(_ tile: FeatureTile) {
		let enabled: [String] = switch tile.scope {
		case .off: []
		case .everywhere: workspaces.map(\.id)
		case .perSphere: workspaces.filter { tile.targets.contains($0.sphere.rawValue) }.map(\.id)
		case .perWorkspace: workspaces.filter { tile.targets.contains($0.id) }.map(\.id)
		}
		do {
			let scope = FeatureScopeView(mode: tile.scope.core, targets: tile.targets.sorted())
			try session.edit { try $0.setFeatureScope(feature: tile.id, scope: scope, enabledWorkspaces: enabled) }
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}
}
