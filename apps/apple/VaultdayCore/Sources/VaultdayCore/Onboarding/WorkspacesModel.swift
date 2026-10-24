import Foundation
import Observation

/// Work or life: the sphere a workspace belongs to.
public enum WorkspaceSphere: String, CaseIterable, Hashable, Sendable {
	case work
	case life

	var core: WorkspaceKindView { self == .work ? .work : .life }

	init(_ kind: WorkspaceKindView) {
		self = kind == .work ? .work : .life
	}
}

/// What a folder is to its workspace.
public enum FolderRole: Hashable, Sendable {
	/// The workspace's own folder, which holds `.vaultday` and takes new notes.
	case root
	/// A folder linked into the root at `link`; it stays where it is.
	case linked(link: String)
	/// A folder the workspace holds a grant for but no link in the root: an
	/// earlier build's further folder, or a link removed by hand or not
	/// carried by a sync service.
	case unlinked
}

/// A folder a workspace reads.
public struct FolderEntry: Identifiable, Hashable, Sendable {
	/// The connection id of the folder's grant.
	public let id: String
	/// The real folder.
	public let path: String
	public let name: String
	public let role: FolderRole
	public let isObsidianVault: Bool

	public var isWorkspaceRoot: Bool { role == .root }

	/// The line under the name.
	public var kindLabel: String {
		switch role {
		case .root: isObsidianVault ? "Obsidian vault" : "Vault folder"
		case .linked: isObsidianVault ? "Linked Obsidian vault" : "Linked folder"
		case .unlinked: "Link missing"
		}
	}
}

/// A workspace in the setup flow's lists.
public struct WorkspaceEntry: Identifiable, Hashable, Sendable {
	/// The workspace id the data key is stored under (`ws-…`).
	public let id: String
	public let name: String
	public let sphere: WorkspaceSphere
	public let accent: String?
	public let rootPath: String
	/// The connection of the root's grant.
	public let rootConnection: String
	/// False once deactivated: still listed in setup and Settings, gone from every other surface.
	public let isActive: Bool
	/// The root, then its linked folders and any grant whose link is missing.
	public let folders: [FolderEntry]
	/// The folders directly in the root that are not links, by name.
	public let subfolders: [String]
	/// The real folders of every link the core could follow in the root,
	/// made here or by hand, for the placement rule.
	public let linkedTargets: [String]

	/// The folders this Mac granted as linked folders: the only links the
	/// core follows when it opens the workspace. A link in the root with no
	/// grant (planted in a synced vault, say) is never followed.
	public var grantedLinkTargets: [String] {
		folders.filter { !$0.isWorkspaceRoot }.map(\.path)
	}

	public init(
		id: String,
		name: String,
		sphere: WorkspaceSphere,
		accent: String?,
		rootPath: String,
		rootConnection: String = "",
		isActive: Bool = true,
		folders: [FolderEntry],
		subfolders: [String] = [],
		linkedTargets: [String] = [],
	) {
		self.id = id
		self.name = name
		self.sphere = sphere
		self.accent = accent
		self.rootPath = rootPath
		self.rootConnection = rootConnection
		self.isActive = isActive
		self.folders = folders
		self.subfolders = subfolders
		self.linkedTargets = linkedTargets
	}
}

/// What a restore found.
public enum RestoreOutcome: Equatable, Sendable {
	case restored(count: Int)
	case nothingFound
	case failed(String)

	/// A line for the Restore popover's footer.
	public var message: String {
		switch self {
		case let .restored(count): count == 1 ? "Restored 1 workspace" : "Restored \(count) workspaces"
		case .nothingFound: "No workspace found in that folder"
		case let .failed(reason): reason
		}
	}
}

/// How a workspace is added on the Workspaces step.
public enum AddWorkspaceChoice: Sendable {
	/// A new workspace in a new or empty folder.
	case create
	/// A folder that already holds `.vaultday`.
	case `import`
}

/// How a folder joins a workspace on the Folders step.
public enum FolderJoin: Sendable {
	/// A symlink in the root; the folder stays where it is.
	case link
	/// The folder moves into the root.
	case move
}

/// Why a picked folder cannot be used, for a popup with a way back.
public struct FolderProblem: Identifiable, Equatable, Sendable {
	public var id: String { message }
	public let title: String
	public let message: String
	/// The workspace the folder clashes with; empty when it clashes with none.
	public let workspaceName: String

	public init(title: String, message: String, workspaceName: String) {
		self.title = title
		self.message = message
		self.workspaceName = workspaceName
	}
}

/// A workspace being added: nothing is written until its Configure sheet's Done.
@MainActor
public final class WorkspaceDraft: Identifiable {
	public let id = UUID()
	/// The folder, canonical.
	public let folder: URL
	/// Whether the folder already held `.vaultday`.
	public let isImport: Bool
	/// The sheet's model; its first save creates the workspace.
	public let configure: WorkspaceConfigureModel
	let picked: URL

	init(folder: URL, picked: URL, isImport: Bool, configure: WorkspaceConfigureModel) {
		self.folder = folder
		self.picked = picked
		self.isImport = isImport
		self.configure = configure
	}

	/// Whether Done created the workspace.
	public var isCommitted: Bool { !configure.isDraft }
}

public enum WorkspaceDraftOutcome {
	case draft(WorkspaceDraft)
	case refused(FolderProblem)
	case failed(String)
}

/// A folder that may join a workspace, picked or moving from another one;
/// the user picks link or move.
public struct PendingFolder: Identifiable, Sendable {
	public let id = UUID()
	public let workspaceId: String
	public let workspaceName: String
	/// The folder, canonical.
	public let folder: URL
	let picked: URL
	/// The workspace the folder leaves, and how it holds it, for a move between workspaces.
	let leaving: (workspaceId: String, origin: FolderOrigin)?
	/// False for a folder inside another root: it can only move.
	public let canLink: Bool
	public var name: String { folder.lastPathComponent }

	init(workspaceId: String, workspaceName: String, folder: URL, picked: URL, leaving: (workspaceId: String, origin: FolderOrigin)? = nil, canLink: Bool = true) {
		self.workspaceId = workspaceId
		self.workspaceName = workspaceName
		self.folder = folder
		self.picked = picked
		self.leaving = leaving
		self.canLink = canLink
	}
}

public enum FolderOffer {
	case ready(PendingFolder)
	case refused(FolderProblem)
	case failed(String)
}

/// A further folder of an earlier build waiting for the one-time question.
public struct UnconvertedFolder: Identifiable, Equatable, Sendable {
	public var id: String { connection }
	public let connection: String
	public let workspaceId: String
	public let workspaceName: String
	public let path: String
}

/// The workspaces of this Mac as the setup steps edit them. Each is one root
/// folder holding `.vaultday/settings.json`, granted through a
/// security-scoped bookmark and recorded in the app-level document; other
/// folders join it as links in the root (with a grant of their own for the
/// sandbox) or by moving into it. Writes to a manifest go through the core;
/// the app document keeps the grants, connections and sources.
@MainActor
@Observable
public final class WorkspacesModel {
	public private(set) var entries: [WorkspaceEntry] = []
	/// Grants that no longer resolve, for the user to grant again.
	public private(set) var problems: [GrantProblem] = []
	/// Further folders of an earlier build, asked about once per device.
	public private(set) var pendingConversion: [UnconvertedFolder] = []
	/// The last refusal or failure, in words for the user.
	public private(set) var message: String?
	/// Called after the list changed, so the open workspace can follow.
	public var onChange: (() -> Void)?

	private let session: CoreSession

	public init(session: CoreSession) {
		self.session = session
		reload()
	}

	/// The workspaces every surface but setup and Settings shows.
	public var activeEntries: [WorkspaceEntry] { entries.filter(\.isActive) }

	// MARK: Reading

	/// Reads the grants, connections and sources of the app document again and
	/// opens access to every folder that still resolves.
	public func reload() {
		let appState = session.appState
		var urls: [String: URL] = [:]
		var found: [GrantProblem] = []
		for grant in appState.folderGrants() {
			switch session.environment.grants.resolve(grant.bookmark) {
			case let .resolved(url, refreshed):
				let folder = Self.canonical(url)
				session.environment.files.allow(folder)
				urls[grant.connection] = folder
				if let refreshed {
					try? session.edit { try $0.putFolderGrant(grant: FolderGrantView(connection: grant.connection, bookmark: refreshed, path: folder.path)) }
				}
			case let .unresolvable(reason):
				found.append(GrantProblem(id: grant.connection, path: grant.path, reason: reason))
			}
		}
		problems = found
		let folderConnections = Set(appState.connections().filter { $0.kind == .folder }.map(\.id))
		var connectionsByWorkspace: [String: [String]] = [:]
		for source in appState.allSources() where folderConnections.contains(source.connection) && urls[source.connection] != nil {
			connectionsByWorkspace[source.workspace, default: []].append(source.connection)
		}
		entries = connectionsByWorkspace
			.compactMap { workspace, connections in makeEntry(id: workspace, connections: connections, urls: urls) }
			.sorted { ($0.sphere == .work ? 0 : 1, $0.name, $0.id) < ($1.sphere == .work ? 0 : 1, $1.name, $1.id) }
		refreshPendingConversion()
	}

	public func entry(id: String) -> WorkspaceEntry? {
		entries.first { $0.id == id }
	}

	/// The workspaces of one sphere, in the order the step lists them.
	public func entries(in sphere: WorkspaceSphere) -> [WorkspaceEntry] {
		entries.filter { $0.sphere == sphere }
	}

	/// The model behind the Configure sheet of a workspace.
	public func configureModel(for workspaceId: String) -> WorkspaceConfigureModel? {
		guard let entry = entry(id: workspaceId) else { return nil }
		return WorkspaceConfigureModel(root: entry.rootPath)
	}

	// MARK: Adding a workspace

	/// Checks a picked folder and opens a draft for its Configure sheet.
	/// Nothing is written: the sheet's Done creates the workspace, and
	/// ``cancel(_:)`` drops the draft. Create takes a new or empty folder
	/// (hidden files and a `Workspace.md` do not count) and imports a folder
	/// that already holds `.vaultday`; Import takes only such a folder.
	public func beginWorkspace(folder picked: URL, sphere: WorkspaceSphere, choice: AddWorkspaceChoice) -> WorkspaceDraftOutcome {
		message = nil
		let url = Self.canonical(picked)
		if session.environment.layout.isInside(url) {
			return .failed("\(url.path) holds Vaultday's own container; pick the workspace folder itself")
		}
		session.environment.grants.startAccessing(picked)
		session.environment.files.allow(url)
		let outcome = draftOutcome(folder: url, picked: picked, sphere: sphere, choice: choice)
		if case .draft = outcome { return outcome }
		releaseAccess(picked: picked, folder: url)
		return outcome
	}

	/// Drops a draft that was never confirmed; the folder is left as it was.
	public func cancel(_ draft: WorkspaceDraft) {
		guard !draft.isCommitted else { return }
		releaseAccess(picked: draft.picked, folder: draft.folder)
	}

	/// Adds a workspace at once, as a draft confirmed with its own values:
	/// the open panel of an empty Today column, and tests.
	///
	/// - Returns: The workspace, or `nil` with ``message`` saying why not.
	@discardableResult
	public func addWorkspace(folder picked: URL, sphere: WorkspaceSphere, name: String? = nil) -> WorkspaceEntry? {
		switch beginWorkspace(folder: picked, sphere: sphere, choice: .create) {
		case let .draft(draft):
			if let name, !name.trimmingCharacters(in: .whitespaces).isEmpty, !draft.isImport {
				draft.configure.name = name
			}
			guard draft.configure.save() else {
				message = draft.configure.errorMessage ?? draft.configure.issues.values.sorted().first
				cancel(draft)
				return nil
			}
			return entries.first { $0.rootPath == draft.folder.path }
		case let .refused(problem):
			message = problem.message
			return nil
		case let .failed(reason):
			message = reason
			return nil
		}
	}

	// MARK: Folders

	/// Checks a folder picked on the Folders step for a workspace; a folder
	/// that may join comes back pending, for the user to link or move it.
	public func offerFolder(_ picked: URL, to workspaceId: String) -> FolderOffer {
		message = nil
		guard let workspace = entry(id: workspaceId) else { return .failed("That workspace is gone") }
		let url = Self.canonical(picked)
		if session.environment.layout.isInside(url) {
			return .failed("\(url.path) holds Vaultday's own container; pick a folder of notes")
		}
		session.environment.grants.startAccessing(picked)
		session.environment.files.allow(url)
		do {
			if let refusal = try checkFolderPlacement(candidate: url.path, workspaceId: workspaceId, workspaces: placedWorkspaces(), leavingRoot: nil) {
				releaseAccess(picked: picked, folder: url)
				return .refused(Self.problem(refusal, folderName: url.lastPathComponent))
			}
		} catch {
			releaseAccess(picked: picked, folder: url)
			return .failed(coreErrorMessage(error))
		}
		return .ready(PendingFolder(workspaceId: workspaceId, workspaceName: workspace.name, folder: url, picked: picked))
	}

	/// Offers a folder of one workspace to another, as a drop or the row's
	/// Move to menu does: a linked folder may be linked or moved, a plain
	/// folder inside the root may only move, and the root itself stays.
	public func offerMove(_ origin: FolderOrigin, from sourceId: String, to targetId: String) -> FolderOffer {
		message = nil
		guard let source = entry(id: sourceId), let target = entry(id: targetId), source.id != target.id else {
			return .failed("Pick another workspace to move the folder to")
		}
		let folder: URL
		switch origin {
		case let .linked(folderId):
			guard let held = source.folders.first(where: { $0.id == folderId }), !held.isWorkspaceRoot else {
				return .failed("A workspace's own folder stays its root; move the folders inside it instead")
			}
			folder = URL(filePath: held.path, directoryHint: .isDirectory)
		case let .inside(name):
			guard source.subfolders.contains(name) else {
				return .failed("\(name) is not a folder of \(source.name)")
			}
			folder = URL(filePath: source.rootPath, directoryHint: .isDirectory).appending(path: name, directoryHint: .isDirectory)
		}
		let placed = placedWorkspaces().compactMap { placed -> PlacedWorkspaceView? in
			guard placed.id == source.id else { return placed }
			guard case .linked = origin else { return nil }
			return PlacedWorkspaceView(id: placed.id, name: placed.name, kind: placed.kind, root: placed.root, linkedTargets: placed.linkedTargets.filter { $0 != folder.path })
		}
		do {
			let leavingRoot: String? = if case .inside = origin { source.rootPath } else { nil }
			if let refusal = try checkFolderPlacement(candidate: folder.path, workspaceId: target.id, workspaces: placed, leavingRoot: leavingRoot) {
				return .refused(Self.problem(refusal, folderName: folder.lastPathComponent))
			}
		} catch {
			return .failed(coreErrorMessage(error))
		}
		let canLink = if case .linked = origin { true } else { false }
		return .ready(PendingFolder(workspaceId: target.id, workspaceName: target.name, folder: folder, picked: folder, leaving: (source.id, origin), canLink: canLink))
	}

	/// Drops a pending folder the user did not link or move.
	public func cancel(_ pending: PendingFolder) {
		guard pending.leaving == nil else { return }
		releaseAccess(picked: pending.picked, folder: pending.folder)
	}

	/// Links a pending folder into its workspace's root and keeps a grant to
	/// it, since the sandbox checks the link's target.
	///
	/// - Returns: Whether the folder was linked; ``message`` says why not.
	@discardableResult
	public func linkFolder(_ pending: PendingFolder) -> Bool {
		message = nil
		guard let workspace = entry(id: pending.workspaceId) else { return false }
		guard pending.canLink else {
			message = "\(pending.name) lies inside another workspace's folder, so it can only move"
			return false
		}
		let root = URL(filePath: workspace.rootPath, directoryHint: .isDirectory)
		var link: URL?
		var recorded: String?
		do {
			let bookmark = try session.environment.grants.makeBookmark(for: pending.picked)
			let made = try makeLink(in: root, to: pending.folder)
			link = made
			recorded = try record(folder: pending.folder, bookmark: bookmark, workspaceId: workspace.id)
			// The old link goes only once the new one stands.
			if let leaving = pending.leaving {
				try release(leaving.origin, of: leaving.workspaceId)
			}
		} catch {
			// The folder stays where it was: in no workspace, or in the one it was leaving.
			if let link { try? FileManager.default.removeItem(at: link) }
			if let recorded { try? session.edit { try $0.removeConnection(id: recorded) } }
			if pending.leaving == nil { releaseAccess(picked: pending.picked, folder: pending.folder) }
			message = Self.unlinkMessage(error, folderName: pending.name)
			changed()
			return false
		}
		changed()
		return true
	}

	/// Moves a pending folder into its workspace's root; it needs no grant of its own there.
	///
	/// - Returns: Whether the folder was moved; ``message`` says why not.
	@discardableResult
	public func moveFolder(_ pending: PendingFolder) -> Bool {
		message = nil
		guard let workspace = entry(id: pending.workspaceId) else { return false }
		let root = URL(filePath: workspace.rootPath, directoryHint: .isDirectory)
		defer { if pending.leaving == nil { releaseAccess(picked: pending.picked, folder: pending.folder) } }
		do {
			try moveIntoRoot(pending.folder, root: root)
		} catch {
			message = Self.moveMessage(error, folder: pending.folder, workspaceName: workspace.name)
			return false
		}
		if let leaving = pending.leaving, case .linked = leaving.origin {
			// The folder is inside the new root now: the old link and grant go.
			do {
				try release(leaving.origin, of: leaving.workspaceId)
			} catch {
				message = coreErrorMessage(error)
			}
		}
		changed()
		return true
	}

	/// Makes an empty folder directly in the workspace's root.
	///
	/// - Returns: Whether the folder was made; ``message`` says why not.
	@discardableResult
	public func createFolder(named rawName: String, in workspaceId: String) -> Bool {
		message = nil
		guard let workspace = entry(id: workspaceId) else { return false }
		let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
		guard Self.isPlainFolderName(name) else {
			message = "A folder name cannot be empty, start with a dot or hold a slash"
			return false
		}
		let folder = URL(filePath: workspace.rootPath, directoryHint: .isDirectory).appending(path: name, directoryHint: .isDirectory)
		guard !FileManager.default.fileExists(atPath: folder.path) else {
			message = "\(workspace.name) already has a folder named \(name)"
			return false
		}
		do {
			try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
		} catch {
			message = coreErrorMessage(error)
			return false
		}
		changed()
		return true
	}

	/// Takes a linked folder out of its workspace: the link and its grant go,
	/// the folder and its notes stay. A grant whose link is already missing goes too.
	public func unlinkFolder(_ folderId: String, from workspaceId: String) {
		message = nil
		guard let folder = entry(id: workspaceId)?.folders.first(where: { $0.id == folderId }), !folder.isWorkspaceRoot else { return }
		do {
			if case let .linked(link) = folder.role {
				try Self.removeLink(atPath: link)
			}
			try session.edit { try $0.removeConnection(id: folderId) }
		} catch {
			message = Self.unlinkMessage(error, folderName: folder.name)
			return
		}
		// The folder left the workspace: its access closes even though the list still names it.
		session.environment.grants.stopAccessing(URL(filePath: folder.path))
		session.environment.files.disallow(URL(filePath: folder.path))
		changed()
	}

	/// Makes the link again for a folder whose link is missing; its grant stays.
	@discardableResult
	public func relinkFolder(_ folderId: String, in workspaceId: String) -> Bool {
		message = nil
		guard let workspace = entry(id: workspaceId),
		      let folder = workspace.folders.first(where: { $0.id == folderId }), folder.role == .unlinked
		else { return false }
		do {
			if let refusal = try placementRefusal(of: folder, in: workspace) {
				message = refusal.message
				return false
			}
			_ = try makeLink(in: URL(filePath: workspace.rootPath, directoryHint: .isDirectory), to: URL(filePath: folder.path, directoryHint: .isDirectory))
		} catch {
			message = coreErrorMessage(error)
			return false
		}
		changed()
		return true
	}

	/// Moves a workspace to the other sphere by changing its kind in
	/// `settings.json`; the app document keeps no kind. A move to Work that
	/// would nest two client workspaces is refused, as a new root would be.
	///
	/// - Returns: The refusal, or `nil` when the workspace moved or was already there.
	@discardableResult
	public func moveWorkspace(_ workspaceId: String, to sphere: WorkspaceSphere) -> FolderProblem? {
		message = nil
		guard let workspace = entry(id: workspaceId), workspace.sphere != sphere else { return nil }
		let others = placedWorkspaces().filter { $0.id != workspace.id }
		if let refusal = checkWorkspaceRootPlacement(candidate: workspace.rootPath, kind: sphere.core, workspaces: others) {
			return Self.problem(refusal, folderName: workspace.name)
		}
		do {
			let form = try readWorkspaceForm(root: workspace.rootPath)
			let moved = WorkspaceFormView(
				name: form.name,
				kind: sphere.core,
				accent: form.accent,
				ticketPattern: form.ticketPattern,
				ticketUrl: form.ticketUrl,
				hosts: form.hosts,
				browser: form.browser,
				hoursPerDay: form.hoursPerDay,
				logFolder: form.logFolder,
				storyTemplate: form.storyTemplate,
			)
			_ = try updateWorkspaceForm(root: workspace.rootPath, form: moved)
		} catch {
			return FolderProblem(title: "\(workspace.name) did not move", message: coreErrorMessage(error), workspaceName: workspace.name)
		}
		changed()
		return nil
	}

	// MARK: Converting an earlier build's folders

	/// Answers the one-time question for every further folder of an earlier
	/// build: link each into its root, or move each into it. A folder already
	/// inside its root only loses its grant; one that cannot join (it belongs
	/// to another workspace, or a move crosses disks) is left as it was and
	/// named in ``message``. The question is not asked again on this Mac.
	public func convertFolders(_ join: FolderJoin) {
		message = nil
		var leftOut: [String] = []
		for pending in pendingConversion {
			guard let workspace = entry(id: pending.workspaceId),
			      let folder = workspace.folders.first(where: { $0.id == pending.connection })
			else { continue }
			do {
				try convert(folder, of: workspace, by: join)
			} catch {
				leftOut.append(URL(filePath: folder.path).lastPathComponent)
			}
		}
		do {
			try session.edit { try $0.setFoldersConverted() }
		} catch {
			message = coreErrorMessage(error)
		}
		if !leftOut.isEmpty {
			message = "Left as they were: \(leftOut.joined(separator: ", "))"
		}
		changed()
	}

	// MARK: Activating

	/// Deactivates or reactivates a workspace. An inactive one keeps its
	/// grant and its place in the app document but leaves every surface,
	/// planning view and the CLI; the open workspace follows through ``onChange``.
	public func setActive(_ isActive: Bool, workspaceId: String) {
		message = nil
		guard let workspace = entry(id: workspaceId), workspace.isActive != isActive else { return }
		do {
			try setWorkspaceActive(root: workspace.rootPath, isActive: isActive)
		} catch {
			message = coreErrorMessage(error)
			return
		}
		changed()
	}

	// MARK: Removing

	/// Removes the workspace from Vaultday on every device: its folders and
	/// grants leave the app document, its data key is destroyed everywhere
	/// and its change files, now unreadable, are moved aside so the folder can
	/// be added again. The notes and links are not touched.
	public func removeWorkspace(id workspaceId: String) {
		message = nil
		guard let workspace = entry(id: workspaceId) else { return }
		do {
			try session.edit { appState in
				for folder in workspace.folders {
					try appState.removeConnection(id: folder.id)
				}
			}
			try deleteWorkspaceKeys(secrets: session.environment.secrets, workspaceId: workspace.id)
			try SyncFolderArchive.archive(root: URL(filePath: workspace.rootPath))
		} catch {
			message = coreErrorMessage(error)
			reload()
			return
		}
		for folder in workspace.folders {
			let url = URL(filePath: folder.path)
			session.environment.files.disallow(url)
			session.environment.grants.stopAccessing(url)
		}
		changed()
	}

	// MARK: Restoring

	/// Takes every workspace found at or below a folder: the folder of an
	/// earlier install or another Mac, a backup, or a place in iCloud Drive.
	public func restore(from picked: URL) -> RestoreOutcome {
		message = nil
		let url = Self.canonical(picked)
		let grants = session.environment.grants
		grants.startAccessing(picked)
		session.environment.files.allow(url)
		defer { grants.stopAccessing(picked) }
		let roots: [WorkspaceBriefView]
		do {
			roots = try discoverWorkspaces(folders: [url.path])
		} catch {
			return .failed(coreErrorMessage(error))
		}
		var added = 0
		var firstFailure: String?
		for brief in roots {
			let root = URL(filePath: brief.root)
			if let refusal = checkWorkspaceRootPlacement(candidate: root.path, kind: brief.kind, workspaces: placedWorkspaces()) {
				if refusal.kind != .alreadyAWorkspace { firstFailure = firstFailure ?? refusal.message }
				continue
			}
			do {
				grants.startAccessing(root)
				session.environment.files.allow(root)
				_ = try register(root: root, bookmark: grants.makeBookmark(for: root))
				added += 1
				reload()
			} catch {
				firstFailure = firstFailure ?? coreErrorMessage(error)
			}
		}
		reload()
		onChange?()
		if added > 0 { return .restored(count: added) }
		if let firstFailure { return .failed(firstFailure) }
		return .nothingFound
	}

	// MARK: Helpers

	private func draftOutcome(folder url: URL, picked: URL, sphere: WorkspaceSphere, choice: AddWorkspaceChoice) -> WorkspaceDraftOutcome {
		do {
			let isImport = try isWorkspaceFolder(path: url.path)
			if choice == .import, !isImport {
				return .refused(FolderProblem(
					title: "\(url.lastPathComponent) holds no workspace",
					message: "Import takes a folder that already holds a .vaultday folder. To start one here, create a workspace instead",
					workspaceName: "",
				))
			}
			if !isImport, !Self.isEmptyFolder(url) {
				return .refused(FolderProblem(
					title: "\(url.lastPathComponent) is not empty",
					message: "A new workspace starts in an empty folder. Choose an empty one, or import a folder that already holds a .vaultday folder; folders of notes join the workspace on the Folders step",
					workspaceName: "",
				))
			}
			let form = isImport ? try readWorkspaceForm(root: url.path) : try newWorkspaceForm(root: url.path, kind: sphere.core)
			if let refusal = checkWorkspaceRootPlacement(candidate: url.path, kind: form.kind, workspaces: placedWorkspaces()) {
				return .refused(Self.problem(refusal, folderName: url.lastPathComponent))
			}
			let configure = WorkspaceConfigureModel(draft: form, root: url.path) { [weak self] form in
				guard let self else { throw CancellationError() }
				try self.commitDraft(folder: url, picked: picked, isImport: isImport, form: form)
			}
			return .draft(WorkspaceDraft(folder: url, picked: picked, isImport: isImport, configure: configure))
		} catch {
			return .failed(coreErrorMessage(error))
		}
	}

	/// The Done of a draft: the manifest, the workspace id and the app document's records.
	private func commitDraft(folder: URL, picked: URL, isImport: Bool, form: WorkspaceFormView) throws {
		let root = folder.path
		let vaultdayFolder = folder.appending(path: ".vaultday", directoryHint: .isDirectory)
		let hadVaultdayFolder = FileManager.default.fileExists(atPath: vaultdayFolder.path)
		if isImport {
			if try readWorkspaceForm(root: root) != form {
				_ = try updateWorkspaceForm(root: root, form: form)
			}
		} else {
			_ = try createWorkspaceFromForm(root: root, form: form)
		}
		do {
			_ = try register(root: folder, bookmark: session.environment.grants.makeBookmark(for: picked))
		} catch {
			if !isImport {
				// Leave the folder as Cancel would: the manifest goes, and the
				// .vaultday folder too unless something else was in it before.
				try? FileManager.default.removeItem(at: hadVaultdayFolder ? vaultdayFolder.appending(path: "settings.json") : vaultdayFolder)
			}
			throw error
		}
		changed()
	}

	private func register(root: URL, bookmark: Data) throws -> String {
		let id = try claimWorkspaceStateId(files: session.environment.files, workspaceRoot: root.path)
		let connection = Self.newConnectionId()
		try session.edit { appState in
			try appState.putConnection(connection: ConnectionView(id: connection, kind: .folder, label: root.lastPathComponent))
			try appState.putFolderGrant(grant: FolderGrantView(connection: connection, bookmark: bookmark, path: root.path))
			try appState.addSource(source: SourceView(workspace: id, connection: connection, subpath: ""))
		}
		return id
	}

	/// Records the grant of a linked folder's target.
	/// Stores a linked folder's connection, grant and source; returns the connection's id.
	@discardableResult
	private func record(folder: URL, bookmark: Data, workspaceId: String) throws -> String {
		let connection = Self.newConnectionId()
		try session.edit { appState in
			try appState.putConnection(connection: ConnectionView(id: connection, kind: .folder, label: folder.lastPathComponent))
			try appState.putFolderGrant(grant: FolderGrantView(connection: connection, bookmark: bookmark, path: folder.path))
			try appState.addSource(source: SourceView(workspace: workspaceId, connection: connection, subpath: ""))
		}
		return connection
	}

	private func convert(_ folder: FolderEntry, of workspace: WorkspaceEntry, by join: FolderJoin) throws {
		let root = URL(filePath: workspace.rootPath, directoryHint: .isDirectory)
		let url = URL(filePath: folder.path, directoryHint: .isDirectory)
		if url.path.hasPrefix(root.path + "/") {
			try session.edit { try $0.removeConnection(id: folder.id) }
			return
		}
		if let refusal = try placementRefusal(of: folder, in: workspace) {
			throw ConversionRefused(message: refusal.message)
		}
		switch join {
		case .link:
			_ = try makeLink(in: root, to: url)
		case .move:
			try moveIntoRoot(url, root: root)
			// Moved is done: a grant that fails to go is only stale.
			try? session.edit { try $0.removeConnection(id: folder.id) }
		}
	}

	/// The placement rule for a folder this workspace already holds a grant
	/// for, checked against the others and the rest of its own.
	private func placementRefusal(of folder: FolderEntry, in workspace: WorkspaceEntry) throws -> FolderRefusalView? {
		let others = placedWorkspaces().map { placed in
			guard placed.id == workspace.id else { return placed }
			return PlacedWorkspaceView(id: placed.id, name: placed.name, kind: placed.kind, root: placed.root, linkedTargets: placed.linkedTargets.filter { $0 != folder.path })
		}
		return try checkFolderPlacement(candidate: folder.path, workspaceId: workspace.id, workspaces: others, leavingRoot: nil)
	}

	/// Takes a folder out of the workspace it leaves: a link and its grant go;
	/// a plain folder inside the root needs nothing, since the move takes it.
	private func release(_ origin: FolderOrigin, of workspaceId: String) throws {
		guard case let .linked(folderId) = origin,
		      let folder = entry(id: workspaceId)?.folders.first(where: { $0.id == folderId })
		else { return }
		if case let .linked(link) = folder.role {
			try Self.removeLink(atPath: link)
		}
		try session.edit { try $0.removeConnection(id: folderId) }
	}

	/// Removes the symlink at `path`, and nothing else: a real folder that
	/// took the link's place since the last reload is refused, never deleted.
	private static func removeLink(atPath path: String) throws {
		let type = try? FileManager.default.attributesOfItem(atPath: path)[.type] as? FileAttributeType
		guard type == .typeSymbolicLink else {
			// Gone already is fine; anything else is not a link.
			if FileManager.default.fileExists(atPath: path) || type != nil { throw NotALink() }
			return
		}
		try FileManager.default.removeItem(atPath: path)
	}

	private struct NotALink: Error {}

	private static func unlinkMessage(_ error: Error, folderName: String) -> String {
		if error is NotALink {
			return "\(folderName) in the workspace's folder is no longer a link, so Vaultday left it alone"
		}
		return coreErrorMessage(error)
	}

	private func makeLink(in root: URL, to folder: URL) throws -> URL {
		let link = Self.freeName(for: folder.lastPathComponent, in: root)
		try FileManager.default.createSymbolicLink(at: link, withDestinationURL: folder)
		return link
	}

	/// Moves a folder into the root under a free name. A move across disks
	/// would copy and delete the notes, so it is refused instead.
	private func moveIntoRoot(_ folder: URL, root: URL) throws {
		let folderVolume = try folder.resourceValues(forKeys: [.volumeIdentifierKey]).volumeIdentifier
		let rootVolume = try root.resourceValues(forKeys: [.volumeIdentifierKey]).volumeIdentifier
		if let folderVolume, let rootVolume, !folderVolume.isEqual(rootVolume) {
			throw MoveError.otherDisk
		}
		try FileManager.default.moveItem(at: folder, to: Self.freeName(for: folder.lastPathComponent, in: root))
	}

	private enum MoveError: Error { case otherDisk }

	private struct ConversionRefused: Error {
		let message: String
	}

	private static func moveMessage(_ error: Error, folder: URL, workspaceName: String) -> String {
		if case MoveError.otherDisk = error {
			return "\(folder.lastPathComponent) is on another disk than \(workspaceName); link it instead"
		}
		return coreErrorMessage(error)
	}

	private func placedWorkspaces() -> [PlacedWorkspaceView] {
		entries.map { entry in
			PlacedWorkspaceView(
				id: entry.id,
				name: entry.name,
				kind: entry.sphere.core,
				root: entry.rootPath,
				linkedTargets: Array(Set(entry.folders.filter { !$0.isWorkspaceRoot }.map(\.path) + entry.linkedTargets)).sorted(),
			)
		}
	}

	private func refreshPendingConversion() {
		guard !session.appState.foldersConverted() else {
			pendingConversion = []
			return
		}
		pendingConversion = entries.flatMap { entry in
			entry.folders.filter { $0.role == .unlinked }.map {
				UnconvertedFolder(connection: $0.id, workspaceId: entry.id, workspaceName: entry.name, path: $0.path)
			}
		}
		if pendingConversion.isEmpty {
			// Nothing of an earlier build here: the question never comes. A
			// grant that does not resolve now is not waited for: when it
			// resolves again it shows as a missing link, and a link this build
			// made and lost later must never be taken for one to move.
			try? session.edit { try $0.setFoldersConverted() }
		}
	}

	private func changed() {
		reload()
		onChange?()
	}

	/// Closes the access a refused or dropped pick opened, unless a workspace
	/// still reads that folder: its root or a linked folder keeps its access.
	private func releaseAccess(picked: URL, folder: URL) {
		let isHeld = entries.contains { entry in
			entry.folders.contains { $0.path == folder.path } || entry.linkedTargets.contains(folder.path)
		}
		guard !isHeld else { return }
		session.environment.grants.stopAccessing(picked)
		session.environment.files.disallow(folder)
	}

	private func makeEntry(id: String, connections: [String], urls: [String: URL]) -> WorkspaceEntry? {
		let granted: [(connection: String, url: URL, isRoot: Bool)] = connections.compactMap { connection in
			guard let url = urls[connection] else { return nil }
			return (connection, url, (try? isWorkspaceFolder(path: url.path)) ?? false)
		}
		guard let root = granted.filter(\.isRoot).min(by: { $0.url.path < $1.url.path }),
		      let form = try? readWorkspaceForm(root: root.url.path)
		else { return nil }
		let links = (try? workspaceLinkedFolders(root: root.url.path)) ?? []
		// The core spells targets as the file system resolves them (`/private/var`);
		// grants are kept in this model's canonical spelling, so the targets follow it.
		let linkNamesByTarget = Dictionary(
			links.map { (Self.canonical(URL(filePath: $0.target)).path, $0.name) },
			uniquingKeysWith: { first, _ in first },
		)
		let others = granted
			.filter { $0.connection != root.connection }
			.sorted { $0.url.path < $1.url.path }
			.map { folder in
				let role: FolderRole = linkNamesByTarget[folder.url.path].map { .linked(link: root.url.appending(path: $0).path) } ?? .unlinked
				return Self.folderEntry(id: folder.connection, url: folder.url, role: role)
			}
		return WorkspaceEntry(
			id: id,
			name: form.name,
			sphere: WorkspaceSphere(form.kind),
			accent: form.accent,
			rootPath: root.url.path,
			rootConnection: root.connection,
			isActive: (try? workspaceIsActive(root: root.url.path)) ?? true,
			folders: [Self.folderEntry(id: root.connection, url: root.url, role: .root)] + others,
			subfolders: Self.subfolderNames(of: root.url),
			linkedTargets: Array(linkNamesByTarget.keys).sorted(),
		)
	}

	private static func folderEntry(id: String, url: URL, role: FolderRole) -> FolderEntry {
		FolderEntry(
			id: id,
			path: url.path,
			name: url.lastPathComponent,
			role: role,
			isObsidianVault: FileManager.default.fileExists(atPath: url.appending(path: ".obsidian").path),
		)
	}

	/// The plain folders directly in the root: not hidden, not links.
	private static func subfolderNames(of root: URL) -> [String] {
		let keys: Set<URLResourceKey> = [.isDirectoryKey, .isSymbolicLinkKey]
		let children = (try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: Array(keys), options: [.skipsHiddenFiles])) ?? []
		return children
			.filter { child in
				let values = try? child.resourceValues(forKeys: keys)
				return values?.isDirectory == true && values?.isSymbolicLink != true
			}
			.map(\.lastPathComponent)
			.sorted()
	}

	/// Empty for a new workspace: nothing but hidden files and a `Workspace.md`,
	/// whose values prefill the manifest.
	private static func isEmptyFolder(_ folder: URL) -> Bool {
		let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
		return names.allSatisfy { $0.hasPrefix(".") || $0 == "Workspace.md" }
	}

	private static func isPlainFolderName(_ name: String) -> Bool {
		!name.isEmpty && !name.hasPrefix(".") && !name.contains("/") && !name.contains(":")
	}

	/// `name`, or `name 2`, `name 3`… when the root already holds it.
	private static func freeName(for name: String, in root: URL) -> URL {
		var candidate = root.appending(path: name, directoryHint: .isDirectory)
		var number = 2
		while (try? candidate.checkResourceIsReachable()) == true || (try? FileManager.default.destinationOfSymbolicLink(atPath: candidate.path)) != nil {
			candidate = root.appending(path: "\(name) \(number)", directoryHint: .isDirectory)
			number += 1
		}
		return candidate
	}

	private static func problem(_ refusal: FolderRefusalView, folderName: String) -> FolderProblem {
		let workspace = refusal.workspaceName
		let title = switch refusal.kind {
		case .alreadyAWorkspace: "\(folderName) is already \(workspace)"
		case .alreadyInWorkspace: "\(folderName) is already in \(workspace)"
		case .belongsTo: "\(folderName) belongs to \(workspace)"
		case .holdsWorkspace: "\(folderName) holds files of \(workspace)"
		case .overlappingWork: "\(folderName) overlaps \(workspace)"
		}
		return FolderProblem(title: title, message: refusal.message, workspaceName: workspace)
	}

	private static func newConnectionId() -> String {
		"con-\(UUID().uuidString.prefix(8).lowercased())"
	}

	/// One spelling per folder (`/var` is `/private/var`), so the file port's
	/// grant and the core's paths agree.
	static func canonical(_ url: URL) -> URL {
		url.standardizedFileURL.resolvingSymlinksInPath()
	}
}
