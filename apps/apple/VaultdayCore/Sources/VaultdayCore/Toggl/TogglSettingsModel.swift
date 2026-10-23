import Foundation
import Observation

/// A story's Toggl project, as typed in Settings.
public struct TogglStoryProject: Identifiable, Hashable, Sendable {
	public var story: String
	public var projectId: String

	public var id: String { story }

	public init(story: String, projectId: String) {
		self.story = story
		self.projectId = projectId
	}
}

/// A Toggl workspace or project offered for picking.
public struct TogglChoice: Identifiable, Hashable, Sendable {
	public let id: UInt64
	public let name: String
}

/// The Toggl and billing panel of Settings for one workspace: the switch,
/// the token, and where entries go in Toggl.
///
/// The token is written to the Keychain on this Mac and nowhere else; the
/// model keeps only the draft being typed, cleared once stored, and then
/// says "Token stored". The mapping is typed by hand; it is fetched from
/// Toggl only while live access is allowed, which this build never allows.
@MainActor
@Observable
public final class TogglSettingsModel {
	public static let tokenStoredText = "Token stored"
	public static let noTokenText = "No token stored"
	public static let typedByHandNote = "Typed by hand until Vaultday may reach Toggl"

	public let workspaceId: String
	public let workspaceName: String
	public private(set) var isOnForApp = false
	public private(set) var isEnabled = false
	public private(set) var hasToken = false
	public private(set) var isLiveAllowed = false
	/// The token being typed; never saved anywhere but the Keychain.
	public var tokenDraft = ""
	public var togglWorkspaceId = ""
	public var defaultProjectId = ""
	public var storyProjects: [TogglStoryProject] = []
	public private(set) var togglWorkspaces: [TogglChoice] = []
	public private(set) var togglProjects: [TogglChoice] = []
	public private(set) var errorMessage: String?

	private let session: CoreSession

	public init(session: CoreSession, workspaceId: String, workspaceName: String) {
		self.session = session
		self.workspaceId = workspaceId
		self.workspaceName = workspaceName
		reload()
	}

	/// The panels to show once the listed workspaces changed while Settings
	/// is open (screen-share mode turned on or off): `fresh` in its order,
	/// keeping the open panel of a workspace still listed, and what is typed in it.
	public static func refreshed(_ current: [TogglSettingsModel], with fresh: [TogglSettingsModel]) -> [TogglSettingsModel] {
		fresh.map { panel in current.first { $0.workspaceId == panel.workspaceId } ?? panel }
	}

	/// "Token stored" or "No token stored": all the panel ever shows of it.
	public var tokenStatusText: String {
		hasToken ? Self.tokenStoredText : Self.noTokenText
	}

	/// Whether the Toggl workspaces and projects may be fetched: the feature
	/// on, a token stored and live access allowed.
	public var canFetchFromToggl: Bool {
		isEnabled && hasToken && isLiveAllowed
	}

	public func reload() {
		do {
			let status = try session.appState.togglStatus(workspace: workspaceId)
			isOnForApp = status.isOnForApp
			isEnabled = status.isEnabled
			hasToken = status.hasToken
			isLiveAllowed = status.isLiveAllowed
			let mapping = session.appState.togglMapping(workspace: workspaceId)
			togglWorkspaceId = mapping.togglWorkspaceId.map(String.init) ?? ""
			defaultProjectId = mapping.defaultProjectId.map(String.init) ?? ""
			storyProjects = mapping.storyProjects.map { TogglStoryProject(story: $0.story, projectId: String($0.projectId)) }
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Switches the feature on for this workspace (and the app), or off for it.
	public func setEnabled(_ isOn: Bool) {
		write { try $0.setTogglEnabled(workspace: workspaceId, isOn: isOn) }
	}

	/// Writes the draft to the Keychain and forgets it.
	public func storeToken() {
		let token = tokenDraft.trimmingCharacters(in: .whitespacesAndNewlines)
		guard !token.isEmpty else {
			errorMessage = "Paste the API token from your Toggl profile"
			return
		}
		do {
			try session.appState.storeTogglToken(workspace: workspaceId, token: token)
			tokenDraft = ""
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reload()
	}

	/// Removes the token from the Keychain.
	public func removeToken() {
		do {
			try session.appState.deleteTogglToken(workspace: workspaceId)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reload()
	}

	public func addStoryProject() {
		storyProjects.append(TogglStoryProject(story: "", projectId: ""))
	}

	/// Saves the mapping as typed. Ids are whole numbers; a blank one is unset.
	@discardableResult
	public func saveMapping() -> Bool {
		guard let workspace = Self.id(togglWorkspaceId), let project = Self.id(defaultProjectId) else {
			errorMessage = "Toggl ids are whole numbers"
			return false
		}
		var projects: [StoryProjectView] = []
		for entry in storyProjects where !entry.story.trimmingCharacters(in: .whitespaces).isEmpty {
			guard let id = UInt64(entry.projectId.trimmingCharacters(in: .whitespaces)) else {
				errorMessage = "The project of \(entry.story) is not a Toggl id"
				return false
			}
			projects.append(StoryProjectView(story: entry.story.trimmingCharacters(in: .whitespaces), projectId: id))
		}
		let mapping = TogglMappingView(togglWorkspaceId: workspace, defaultProjectId: project, storyProjects: projects)
		return write { try $0.setTogglMapping(workspace: workspaceId, mapping: mapping) }
	}

	/// Fetches the account's Toggl workspaces and, for the chosen one, its
	/// projects. Does nothing unless ``canFetchFromToggl``.
	public func fetchFromToggl() {
		guard canFetchFromToggl else { return }
		do {
			togglWorkspaces = try session.appState.fetchTogglWorkspaces(workspace: workspaceId).map { TogglChoice(id: $0.id, name: $0.name) }
			if let chosen = UInt64(togglWorkspaceId) {
				togglProjects = try session.appState.fetchTogglProjects(workspace: workspaceId, togglWorkspaceId: chosen).map { TogglChoice(id: $0.id, name: $0.name) }
			}
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	@discardableResult
	private func write(_ edit: (AppStateHandle) throws -> Void) -> Bool {
		defer { reload() }
		do {
			try session.edit(edit)
			errorMessage = nil
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}

	/// A typed id: `.some(nil)` for blank, `.some(id)` for a number, `nil` otherwise.
	private static func id(_ text: String) -> UInt64?? {
		let trimmed = text.trimmingCharacters(in: .whitespaces)
		if trimmed.isEmpty { return .some(nil) }
		guard let id = UInt64(trimmed) else { return nil }
		return .some(id)
	}
}
