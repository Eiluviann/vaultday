import Foundation

/// Where the app keeps what is not in the vault, inside the App Group
/// container (`~/Library/Group Containers/<group>/`), which the CLI can read
/// too. Nothing here ever goes into a workspace folder.
public struct AppGroupLayout: Sendable, Equatable {
	public let container: URL

	public init(container: URL) {
		self.container = container
	}

	/// The App Group container for `identifier`, `nil` when the app is not entitled to it.
	public static func appGroup(_ identifier: String) -> AppGroupLayout? {
		FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: identifier).map(AppGroupLayout.init)
	}

	/// This device's signed, encrypted app-level snapshots.
	public var appStateFolder: URL { container.appending(path: "AppState", directoryHint: .isDirectory) }
	/// Volatile state: rebuilt from the vault and app state when lost.
	public var volatileFolder: URL { container.appending(path: "Volatile", directoryHint: .isDirectory) }
	/// The parse index, one folder per workspace.
	public var indexFolder: URL { volatileFolder.appending(path: "Index", directoryHint: .isDirectory) }
	/// Caches.
	public var cachesFolder: URL { volatileFolder.appending(path: "Caches", directoryHint: .isDirectory) }
	/// Window state: sidebar collapse, focused pane.
	public var windowStateFile: URL { volatileFolder.appending(path: "window-state.json") }
	/// Reserved for the localhost API's token (PR 11); `vaultday status` reads it here.
	public var apiTokenFile: URL { container.appending(path: "API/token") }

	/// Creates the folders the app writes into.
	public func createFolders() throws {
		for folder in [appStateFolder, indexFolder, cachesFolder] {
			try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
		}
	}

	/// Whether the container sits inside `workspace`, which would put volatile
	/// state into a synced folder; such a workspace is refused.
	public func isInside(_ workspace: URL) -> Bool {
		let root = workspace.standardizedFileURL.resolvingSymlinksInPath().path
		let own = container.standardizedFileURL.resolvingSymlinksInPath().path
		return own == root || own.hasPrefix(root.hasSuffix("/") ? root : root + "/")
	}
}
