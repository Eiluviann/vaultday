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

/// A folder a workspace reads.
public struct FolderEntry: Identifiable, Hashable, Sendable {
	/// The connection id.
	public let id: String
	public let path: String
	public let name: String
	/// The workspace's own folder, which holds `.vaultday` and takes new notes.
	public let isWorkspaceRoot: Bool
	public let isObsidianVault: Bool

	/// The line under the name: "Obsidian vault", "Vault folder" or "Markdown folder".
	public var kindLabel: String {
		if isObsidianVault { return "Obsidian vault" }
		return isWorkspaceRoot ? "Vault folder" : "Markdown folder"
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
	public let folders: [FolderEntry]
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

/// The workspaces of this Mac as the setup steps edit them: each is a folder
/// holding `.vaultday/settings.json`, granted through a security-scoped
/// bookmark and recorded in the app-level document, plus any further folders
/// it reads. Writes to a manifest go through the core; the app document keeps
/// the grants, connections and sources.
@MainActor
@Observable
public final class WorkspacesModel {
	public private(set) var entries: [WorkspaceEntry] = []
	/// Grants that no longer resolve, for the user to grant again.
	public private(set) var problems: [GrantProblem] = []
	/// The last refusal or failure, in words for the user.
	public private(set) var message: String?
	/// Called after the list changed, so the open workspace can follow.
	public var onChange: (() -> Void)?

	private let session: CoreSession

	public init(session: CoreSession) {
		self.session = session
		reload()
	}

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

	// MARK: Adding

	/// Makes a folder a workspace. A folder that already holds
	/// `.vaultday/settings.json` is taken as it is; one that does not gets a
	/// manifest from the core, prefilled once from its `Workspace.md`.
	///
	/// - Returns: The workspace, or `nil` with ``message`` saying why not.
	@discardableResult
	public func addWorkspace(folder picked: URL, sphere: WorkspaceSphere, name: String? = nil) -> WorkspaceEntry? {
		message = nil
		let url = Self.canonical(picked)
		let grants = session.environment.grants
		if session.environment.layout.isInside(url) {
			message = "\(url.path) holds Vaultday's own container; pick the workspace folder itself"
			return nil
		}
		grants.startAccessing(picked)
		session.environment.files.allow(url)
		do {
			let bookmark = try grants.makeBookmark(for: picked)
			let isExisting = try isWorkspaceFolder(path: url.path)
			let form = isExisting ? try readWorkspaceForm(root: url.path) : nil
			let workspaceName = form?.name ?? name ?? url.lastPathComponent
			let kind = form?.kind ?? sphere.core
			try checkFolderGrant(candidate: url.path, workspaceId: nil, workspaceName: workspaceName, kind: kind, existing: claims())
			if !isExisting {
				// No name: the core takes the name from Workspace.md, else from the folder.
				_ = try createWorkspace(root: url.path, name: name ?? "", kind: kind)
			}
			let id = try register(root: url, bookmark: bookmark)
			reload()
			onChange?()
			return entry(id: id)
		} catch {
			grants.stopAccessing(picked)
			session.environment.files.disallow(url)
			message = coreErrorMessage(error)
			return nil
		}
	}

	/// Adds a further folder to a workspace: it reads the folder in place.
	///
	/// - Returns: Whether the folder was added; ``message`` says why not.
	@discardableResult
	public func addFolder(_ picked: URL, to workspaceId: String) -> Bool {
		message = nil
		guard let workspace = entry(id: workspaceId) else { return false }
		let url = Self.canonical(picked)
		let grants = session.environment.grants
		if session.environment.layout.isInside(url) {
			message = "\(url.path) holds Vaultday's own container; pick a folder of notes"
			return false
		}
		grants.startAccessing(picked)
		session.environment.files.allow(url)
		do {
			try checkFolderGrant(candidate: url.path, workspaceId: workspace.id, workspaceName: workspace.name, kind: workspace.sphere.core, existing: claims())
			let bookmark = try grants.makeBookmark(for: picked)
			let connection = Self.newConnectionId()
			try session.edit { appState in
				try appState.putConnection(connection: ConnectionView(id: connection, kind: .folder, label: url.lastPathComponent))
				try appState.putFolderGrant(grant: FolderGrantView(connection: connection, bookmark: bookmark, path: url.path))
				try appState.addSource(source: SourceView(workspace: workspace.id, connection: connection, subpath: ""))
			}
			reload()
			onChange?()
			return true
		} catch {
			grants.stopAccessing(picked)
			session.environment.files.disallow(url)
			message = coreErrorMessage(error)
			return false
		}
	}

	/// Makes a new folder, as the Add-folder popover's second choice does, and adds it.
	@discardableResult
	public func addNewFolder(_ url: URL, to workspaceId: String) -> Bool {
		do {
			try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
		} catch {
			message = coreErrorMessage(error)
			return false
		}
		return addFolder(url, to: workspaceId)
	}

	// MARK: Removing

	/// Drops a further folder from a workspace; the folder and its notes stay.
	/// The workspace's own folder is removed with the workspace.
	public func removeFolder(_ folderId: String, from workspaceId: String) {
		message = nil
		guard let folder = entry(id: workspaceId)?.folders.first(where: { $0.id == folderId }), !folder.isWorkspaceRoot else { return }
		do {
			try session.edit { try $0.removeConnection(id: folderId) }
		} catch {
			message = coreErrorMessage(error)
			return
		}
		let url = URL(filePath: folder.path)
		session.environment.files.disallow(url)
		session.environment.grants.stopAccessing(url)
		reload()
		onChange?()
	}

	/// Removes the workspace from Vaultday on every device: its folders and
	/// grants leave the app document, its data key is destroyed everywhere
	/// and its change files, now unreadable, are moved aside so the folder can
	/// be added again. The notes are not touched.
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
		reload()
		onChange?()
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
			do {
				try checkFolderGrant(candidate: root.path, workspaceId: nil, workspaceName: brief.name, kind: brief.kind, existing: claims())
				grants.startAccessing(root)
				session.environment.files.allow(root)
				_ = try register(root: root, bookmark: grants.makeBookmark(for: root))
				added += 1
			} catch {
				if let message = Optional(coreErrorMessage(error)), !message.contains("already a folder") {
					firstFailure = firstFailure ?? message
				}
			}
		}
		reload()
		onChange?()
		if added > 0 { return .restored(count: added) }
		if let firstFailure { return .failed(firstFailure) }
		return .nothingFound
	}

	// MARK: Helpers

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

	private func claims() -> [FolderClaimView] {
		entries.flatMap { workspace in
			workspace.folders.map {
				FolderClaimView(path: $0.path, workspaceId: workspace.id, workspaceName: workspace.name, kind: workspace.sphere.core)
			}
		}
	}

	private func makeEntry(id: String, connections: [String], urls: [String: URL]) -> WorkspaceEntry? {
		let folders: [(connection: String, url: URL, isRoot: Bool)] = connections.compactMap { connection in
			guard let url = urls[connection] else { return nil }
			return (connection, url, (try? isWorkspaceFolder(path: url.path)) ?? false)
		}
		guard let root = folders.first(where: \.isRoot),
		      let form = try? readWorkspaceForm(root: root.url.path)
		else { return nil }
		let ordered = folders.sorted { ($0.isRoot ? 0 : 1, $0.url.path) < ($1.isRoot ? 0 : 1, $1.url.path) }
		return WorkspaceEntry(
			id: id,
			name: form.name,
			sphere: WorkspaceSphere(form.kind),
			accent: form.accent,
			rootPath: root.url.path,
			folders: ordered.map { folder in
				FolderEntry(
					id: folder.connection,
					path: folder.url.path,
					name: folder.url.lastPathComponent,
					isWorkspaceRoot: folder.isRoot,
					isObsidianVault: FileManager.default.fileExists(atPath: folder.url.appending(path: ".obsidian").path),
				)
			},
		)
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
