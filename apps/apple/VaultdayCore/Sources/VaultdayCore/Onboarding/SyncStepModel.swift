import AppKit
import Foundation
import Observation

/// Where the working copy of the notes lives.
public enum SyncHomeChoice: String, CaseIterable, Hashable, Sendable {
	case local
	case localAndCloud
	case cloud

	var core: SyncHomeView {
		switch self {
		case .local: .local
		case .localAndCloud: .localAndCloud
		case .cloud: .cloud
		}
	}

	init(_ view: SyncHomeView) {
		switch view {
		case .local: self = .local
		case .localAndCloud: self = .localAndCloud
		case .cloud: self = .cloud
		}
	}
}

/// The service that carries the notes between devices.
public enum SyncServiceChoice: String, CaseIterable, Hashable, Sendable {
	case iCloudDrive
	case dropbox
	case googleDrive
	case syncthing
	case folder

	public var title: String {
		switch self {
		case .iCloudDrive: "iCloud Drive"
		case .dropbox: "Dropbox"
		case .googleDrive: "Google Drive"
		case .syncthing: "Syncthing"
		case .folder: "A folder"
		}
	}

	var core: SyncServiceView {
		switch self {
		case .iCloudDrive: .icloudDrive
		case .dropbox: .dropbox
		case .googleDrive: .googleDrive
		case .syncthing: .syncthing
		case .folder: .folder
		}
	}

	init(_ view: SyncServiceView) {
		switch view {
		case .icloudDrive: self = .iCloudDrive
		case .dropbox: self = .dropbox
		case .googleDrive: self = .googleDrive
		case .syncthing: self = .syncthing
		case .folder: self = .folder
		}
	}
}

/// A workspace whose folder sits in a git repository.
public struct GitWorkspace: Identifiable, Hashable, Sendable {
	/// The workspace id.
	public let id: String
	public let workspaceName: String
	/// The `.gitignore` the line goes into: the workspace folder's own.
	public let ignoreFilePath: String
	/// Whether that file already holds the line.
	public let hasLine: Bool
}

/// The Sync and backup step: the mode and the service go into the app-level
/// document; the step explains that `.vaultday/sync` has to travel between
/// devices and offers the `.gitignore` line for vaults kept in git.
@MainActor
@Observable
public final class SyncStepModel {
	/// Why the sync folder matters, under the step's callout.
	public static let syncFolderNote = "Every workspace keeps its encrypted app state in .vaultday/sync; your sync service has to carry that folder to your other devices, or they never see what changed here"
	/// The warning shown when the notes stay on this Mac.
	public static let localOnlyWarning = "You won't be able to use your vaults on your other devices"
	public static let gitNote = "Commit .vaultday/sync with the notes and keep only its temporary files out of git"

	public private(set) var home: SyncHomeChoice
	public private(set) var service: SyncServiceChoice
	public private(set) var isServiceChosen: Bool
	public private(set) var gitWorkspaces: [GitWorkspace] = []
	/// The result of the last copy or append.
	public private(set) var gitMessage: String?
	public private(set) var errorMessage: String?

	private let session: CoreSession
	private let workspaces: WorkspacesModel
	private let copyToPasteboard: @MainActor (String) -> Void

	public init(
		session: CoreSession,
		workspaces: WorkspacesModel,
		copyToPasteboard: @escaping @MainActor (String) -> Void = { text in
			NSPasteboard.general.clearContents()
			NSPasteboard.general.setString(text, forType: .string)
		},
	) {
		self.session = session
		self.workspaces = workspaces
		self.copyToPasteboard = copyToPasteboard
		let choice = session.appState.syncChoice()
		home = SyncHomeChoice(choice.home)
		service = SyncServiceChoice(choice.service)
		isServiceChosen = choice.isServiceChosen
		refreshGitWorkspaces()
	}

	/// The line offered for `.gitignore`.
	public var gitignoreLine: String { gitignoreSnippet() }
	/// Whether the cloud service card shows: not with notes kept on this Mac only.
	public var showsCloudService: Bool { home != .local }
	public var showsLocalOnlyWarning: Bool { home == .local }

	public func setHome(_ choice: SyncHomeChoice) {
		home = choice
		store()
	}

	/// Picks a service in the control; nothing is chosen until ``confirmService()``.
	public func setService(_ choice: SyncServiceChoice) {
		guard choice != service else { return }
		service = choice
		isServiceChosen = false
		store()
	}

	public func confirmService() {
		isServiceChosen = true
		store()
	}

	/// Lists the workspaces that sit in a git repository; run again when the workspaces change.
	public func refreshGitWorkspaces() {
		gitWorkspaces = workspaces.entries.compactMap { entry in
			let root = URL(filePath: entry.rootPath)
			guard Self.isInsideGitRepository(root) else { return nil }
			let ignoreFile = root.appending(path: ".gitignore")
			return GitWorkspace(
				id: entry.id,
				workspaceName: entry.name,
				ignoreFilePath: ignoreFile.path,
				hasLine: (try? gitignoreHasSnippet(workspaceRoot: entry.rootPath)) ?? false,
			)
		}
	}

	public func copyGitignoreLine() {
		copyToPasteboard(gitignoreLine)
		gitMessage = "Copied the line"
	}

	/// Has the core append the line to the workspace folder's `.gitignore`,
	/// creating it, unless it is already there.
	public func appendGitignoreLine(forWorkspace id: String) {
		guard let target = gitWorkspaces.first(where: { $0.id == id }),
		      let root = workspaces.entry(id: id)?.rootPath
		else { return }
		do {
			let isAdded = try addGitignoreSnippet(workspaceRoot: root)
			gitMessage = isAdded ? "Added the line to \(target.workspaceName)'s .gitignore" : ".gitignore already has the line"
		} catch {
			gitMessage = coreErrorMessage(error)
		}
		refreshGitWorkspaces()
	}

	private func store() {
		let choice = SyncChoiceView(home: home.core, service: service.core, isServiceChosen: isServiceChosen)
		do {
			try session.edit { try $0.setSyncChoice(choice: choice) }
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	private static func isInsideGitRepository(_ folder: URL) -> Bool {
		var candidate = folder
		while true {
			if FileManager.default.fileExists(atPath: candidate.appending(path: ".git").path) { return true }
			let parent = candidate.deletingLastPathComponent()
			if parent.path == candidate.path { return false }
			candidate = parent
		}
	}
}
