import Foundation
import Observation

/// A protection level a workspace can have.
public enum SecurityLevel: String, CaseIterable, Identifiable, Hashable, Sendable {
	case off
	case enhanced
	case strict

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .off: "Off"
		case .enhanced: "Enhanced"
		case .strict: "Strict"
		}
	}

	/// What the level means, as the card lists it.
	public var points: [String] {
		switch self {
		case .off:
			["Any model you switch on can read everything", "Cloud models included"]
		case .enhanced:
			["Cloud models, Claude Code included, see compacted mail and messages only", "Originals stay encrypted on this Mac"]
		case .strict:
			["On-device models only", "Claude Code and the vaultday command refuse it", "Opens only after you unlock"]
		}
	}

	var core: SecurityModeView {
		switch self {
		case .off: .off
		case .enhanced: .enhanced
		case .strict: .strict
		}
	}

	init(_ view: SecurityModeView) {
		switch view {
		case .off: self = .off
		case .enhanced: self = .enhanced
		case .strict: self = .strict
		}
	}
}

/// The Security step. The base version enforces no security level, so every
/// workspace stays Off and the controls say Planned. The fields the app
/// document reserved for the levels are the ones written: each workspace's
/// mode is recorded as Off, and a level that would promise protection nothing
/// provides is refused. Touch ID, the recovery key and a hardware key arrive
/// with the levels; nothing secret is shown or stored here.
@MainActor
@Observable
public final class SecurityStepModel {
	/// Whether any level above Off can be chosen in this release.
	public static let areLevelsAvailable = false
	/// The line that says why not.
	public static let plannedNote = "Security levels arrive in a later release; every workspace stays Off until then"

	/// Each workspace's recorded level, by workspace id.
	public private(set) var levels: [String: SecurityLevel] = [:]
	public private(set) var errorMessage: String?

	private let session: CoreSession

	public init(session: CoreSession) {
		self.session = session
	}

	/// Reads the recorded level of each workspace and records Off for one that has none.
	public func refresh(workspaces: [WorkspaceEntry]) {
		for workspace in workspaces {
			let stored = session.appState.security(workspace: workspace.id)
			levels[workspace.id] = SecurityLevel(stored.mode)
			if stored.mode == .off, !stored.alwaysAskBeforeSending {
				record(.off, workspace: workspace.id)
			}
		}
	}

	public func level(of workspace: String) -> SecurityLevel { levels[workspace] ?? .off }

	/// Records a workspace's level. Anything above Off is refused in this release; nothing is written then.
	@discardableResult
	public func setLevel(_ level: SecurityLevel, workspace: String) -> Bool {
		guard level == .off || Self.areLevelsAvailable else {
			errorMessage = Self.plannedNote
			return false
		}
		return record(level, workspace: workspace)
	}

	@discardableResult
	private func record(_ level: SecurityLevel, workspace: String) -> Bool {
		do {
			let security = WorkspaceSecurityView(mode: level.core, alwaysAskBeforeSending: false)
			try session.edit { try $0.setSecurity(workspace: workspace, security: security) }
			levels[workspace] = level
			errorMessage = nil
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}
}
