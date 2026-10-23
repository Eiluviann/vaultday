import CryptoKit
import Foundation
import Observation

/// A folder grant that did not resolve at launch.
public struct GrantProblem: Identifiable, Hashable, Sendable {
	public let id: String
	public let path: String
	public let reason: String
}

/// What the workspace's synced app state says, for the sync check.
public struct SyncSummary: Hashable, Sendable {
	public let deviceId: String
	/// A short digest of the document heads: equal on two Macs exactly when they converged.
	public let headsDigest: String
	/// The probe entries every device wrote with ``WorkspaceModel/writeSyncProbe()``.
	public let probes: [String]
	/// Change files that have not arrived yet.
	public let pendingFiles: Int
}

/// The open workspace: its folder grant, the vault through the core and its
/// encrypted app state. Views read plain values; the core's handles stay
/// inside the package.
@MainActor
@Observable
public final class WorkspaceModel {
	public enum Phase: Equatable, Sendable {
		/// No folder granted yet.
		case noFolder
		case open
		case failed(String)
	}

	/// Where the sync check writes its probes: a day no plan uses.
	static let probeDate = "2000-01-01"

	public private(set) var phase: Phase = .noFolder
	public private(set) var name = ""
	public private(set) var workspaceId = ""
	public private(set) var rootPath = ""
	/// A line about the app state for the window's footer.
	public private(set) var appStateStatus = ""
	public private(set) var grantProblems: [GrantProblem] = []
	public private(set) var syncSummary: SyncSummary?
	/// The calendars this workspace reads, by EventKit calendar identifier.
	public private(set) var pickedCalendarIds: Set<String> = []

	private let session: CoreSession
	private(set) var vault: WorkspaceHandle?
	private(set) var state: WorkspaceStateHandle?
	private var connectionId: String?
	private var root: URL?
	/// The URL security-scoped access was opened on, as the grant resolved it.
	private var accessedFolder: URL?

	public init(session: CoreSession) {
		self.session = session
	}

	/// Resolves the folder grants stored for this device and opens the first
	/// one that still resolves. A stale bookmark is refreshed and stored; one
	/// that no longer resolves is listed in ``grantProblems``.
	public func restoreAtLaunch() {
		grantProblems = []
		for grant in session.appState.folderGrants() {
			switch session.environment.grants.resolve(grant.bookmark) {
			case let .resolved(url, refreshedBookmark):
				if let refreshedBookmark {
					storeGrant(connection: grant.connection, bookmark: refreshedBookmark, path: url.path)
				}
				if phase != .open {
					open(root: url, connection: grant.connection)
				}
				// One workspace is open at a time in this shell; the scopes of the others close.
				if accessedFolder != url {
					session.environment.grants.stopAccessing(url)
				}
			case let .unresolvable(reason):
				grantProblems.append(GrantProblem(id: grant.connection, path: grant.path, reason: reason))
			}
		}
	}

	/// Grants a folder the user picked in the open panel and opens it. The
	/// connection and its grant are stored only when the folder opened as a workspace.
	public func grantFolder(_ url: URL) {
		let grants = session.environment.grants
		grants.startAccessing(url)
		let bookmark: Data
		do {
			bookmark = try grants.makeBookmark(for: url)
		} catch {
			grants.stopAccessing(url)
			phase = .failed(coreErrorMessage(error))
			return
		}
		let connection = "con-\(UUID().uuidString.prefix(8).lowercased())"
		open(root: url, connection: connection)
		guard phase == .open else {
			grants.stopAccessing(url)
			return
		}
		do {
			try session.appState.putConnection(connection: ConnectionView(id: connection, kind: .folder, label: url.lastPathComponent))
			storeGrant(connection: connection, bookmark: bookmark, path: url.path)
		} catch {
			appStateStatus = coreErrorMessage(error)
		}
	}

	/// Removes the workspace from this Mac and every device. The app document
	/// drops its connection, sources and this device's grant first; then the
	/// data key is destroyed everywhere, and the change files it sealed, now
	/// unreadable, are moved aside to `.vaultday/sync-removed-<time>`, so the
	/// folder can be added again as a new workspace. The notes are not touched.
	public func removeWorkspace() {
		do {
			if let connectionId {
				try session.appState.removeConnection(id: connectionId)
				try session.save()
			}
			if !workspaceId.isEmpty {
				try deleteWorkspaceKeys(secrets: session.environment.secrets, workspaceId: workspaceId)
			}
			try archiveSyncFolder()
		} catch {
			phase = .failed(coreErrorMessage(error))
			return
		}
		close()
	}

	/// Picks a calendar for this workspace's reads, or drops it. A pick is a
	/// calendar connection plus a source of this workspace in the app document.
	public func setCalendar(id: String, title: String, isPicked: Bool) {
		guard !workspaceId.isEmpty else { return }
		let source = SourceView(workspace: workspaceId, connection: id, subpath: "")
		do {
			if isPicked {
				try session.appState.putConnection(connection: ConnectionView(id: id, kind: .calendar, label: title))
				try session.appState.addSource(source: source)
			} else {
				try session.appState.removeSource(source: source)
			}
			try session.save()
		} catch {
			appStateStatus = coreErrorMessage(error)
		}
		refreshPickedCalendars()
	}

	/// Applies change files that arrived from other devices.
	public func pull() {
		guard let state else { return }
		do {
			let outcome = try state.pull()
			appStateStatus = statusLine(applied: Int(outcome.applied), pending: outcome.pending.count)
		} catch {
			appStateStatus = coreErrorMessage(error)
		}
		refreshSyncSummary()
	}

	/// The sync check of the two-Mac test: appends this device's id and the
	/// time to the probe day and writes the change file at once.
	public func writeSyncProbe() {
		guard let state else { return }
		do {
			let probes = try state.dayOrder(date: Self.probeDate)
			let stamp = ISO8601DateFormatter().string(from: Date())
			try state.setDayOrder(date: Self.probeDate, taskIds: probes + ["probe-\(session.deviceId.prefix(8))-\(stamp)"])
			_ = try state.flush()
		} catch {
			appStateStatus = coreErrorMessage(error)
		}
		refreshSyncSummary()
	}

	// MARK: Opening

	private func open(root picked: URL, connection: String) {
		// One spelling per folder (`/var` is `/private/var`), so the file
		// port's grant and the core's paths always agree.
		let url = picked.standardizedFileURL.resolvingSymlinksInPath()
		if session.environment.layout.isInside(url) {
			phase = .failed("\(url.path) holds Vaultday's own container; pick the workspace folder itself")
			return
		}
		session.environment.files.allow(url)
		do {
			let vault = try openWorkspace(path: url.path)
			let id = try claimWorkspaceStateId(files: session.environment.files, workspaceRoot: vault.root())
			self.vault = vault
			root = url
			accessedFolder = picked
			connectionId = connection
			name = vault.name()
			workspaceId = id
			rootPath = vault.root()
			try session.appState.addSource(source: SourceView(workspace: workspaceId, connection: connection, subpath: ""))
			try session.save()
			refreshPickedCalendars()
			phase = .open
		} catch {
			session.environment.files.disallow(url)
			phase = .failed(coreErrorMessage(error))
			return
		}
		openAppState()
	}

	/// Opens the workspace's app state, or starts it when no device has.
	private func openAppState() {
		let environment = session.environment
		do {
			let state = try openWorkspaceState(
				files: environment.files,
				workspaceRoot: rootPath,
				workspaceId: workspaceId,
				deviceId: session.deviceId,
				secrets: environment.secrets,
			)
			self.state = state
			let pull = state.lastPull()
			appStateStatus = statusLine(applied: Int(pull.applied), pending: pull.pending.count)
		} catch CoreError.KeyNotArrived(_) {
			startAppState()
		} catch {
			appStateStatus = coreErrorMessage(error)
		}
		refreshSyncSummary()
	}

	private func startAppState() {
		let environment = session.environment
		do {
			state = try createWorkspaceState(
				files: environment.files,
				workspaceRoot: rootPath,
				workspaceId: workspaceId,
				deviceId: session.deviceId,
				secrets: environment.secrets,
			)
			appStateStatus = "App state started on this Mac"
		} catch CoreError.Conflict(_) {
			appStateStatus = "Waiting for this workspace's key from your other Mac (iCloud Keychain)"
		} catch {
			appStateStatus = coreErrorMessage(error)
		}
	}

	/// Closes the open workspace without touching the app document or the folder.
	public func close() {
		if let root {
			session.environment.files.disallow(root)
		}
		if let accessedFolder {
			session.environment.grants.stopAccessing(accessedFolder)
		}
		accessedFolder = nil
		vault = nil
		state = nil
		root = nil
		connectionId = nil
		name = ""
		workspaceId = ""
		rootPath = ""
		appStateStatus = ""
		syncSummary = nil
		pickedCalendarIds = []
		phase = .noFolder
	}

	// MARK: Helpers

	private func archiveSyncFolder() throws {
		guard let root else { return }
		try SyncFolderArchive.archive(root: root)
	}

	private func storeGrant(connection: String, bookmark: Data, path: String) {
		do {
			try session.appState.putFolderGrant(grant: FolderGrantView(connection: connection, bookmark: bookmark, path: path))
			try session.save()
		} catch {
			appStateStatus = coreErrorMessage(error)
		}
	}

	private func refreshPickedCalendars() {
		let calendars = Set(session.appState.connections().filter { $0.kind == .calendar }.map(\.id))
		pickedCalendarIds = Set(session.appState.sources(workspace: workspaceId).map(\.connection)).intersection(calendars)
	}

	private func refreshSyncSummary() {
		guard let state else {
			syncSummary = nil
			return
		}
		let heads = Data(state.heads().sorted().joined(separator: ",").utf8)
		let digest = SHA256.hash(data: heads).map { String(format: "%02x", $0) }.joined()
		syncSummary = SyncSummary(
			deviceId: session.deviceId,
			headsDigest: String(digest.prefix(12)),
			probes: (try? state.dayOrder(date: Self.probeDate)) ?? [],
			pendingFiles: state.lastPull().pending.count,
		)
	}

	private func statusLine(applied: Int, pending: Int) -> String {
		pending == 0
			? "App state up to date, \(applied) change files applied"
			: "App state: \(applied) change files applied, \(pending) not arrived yet"
	}
}
