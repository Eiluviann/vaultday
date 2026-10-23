import Foundation

/// Moves a removed workspace's `.vaultday/sync` folder aside.
enum SyncFolderArchive {
	/// Moves `.vaultday/sync` to `.vaultday/sync-removed-<time>` through file
	/// coordination, so a synced folder is never removed behind a provider's
	/// back. The change files in it are unreadable once the data key is
	/// destroyed; moving them lets the folder be added again as a new workspace.
	static func archive(root: URL) throws {
		let sync = root.appending(path: ".vaultday/sync", directoryHint: .isDirectory)
		guard FileManager.default.fileExists(atPath: sync.path) else { return }
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.dateFormat = "yyyyMMdd-HHmmss"
		let archive = root.appending(path: ".vaultday/sync-removed-\(formatter.string(from: Date()))", directoryHint: .isDirectory)
		var coordinationError: NSError?
		var moveError: Error?
		NSFileCoordinator(filePresenter: nil).coordinate(writingItemAt: sync, options: .forMoving, error: &coordinationError) { url in
			do {
				try FileManager.default.moveItem(at: url, to: archive)
			} catch {
				moveError = error
			}
		}
		if let error = coordinationError ?? moveError { throw error }
	}
}
