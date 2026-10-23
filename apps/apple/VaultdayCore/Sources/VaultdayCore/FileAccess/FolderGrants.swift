import Foundation

/// What resolving a stored grant gave.
public enum GrantResolution: Equatable, Sendable {
	/// The folder is reachable. `refreshedBookmark` is set when the stored
	/// bookmark was stale (the folder moved or was renamed): the caller stores
	/// it in place of the old one.
	case resolved(url: URL, refreshedBookmark: Data?)
	/// The bookmark no longer leads anywhere; the user grants the folder again.
	case unresolvable(reason: String)
}

/// Security-scoped bookmarks for the folders the user granted through the
/// open panel. Bookmarks are made with `.withSecurityScope` so the sandboxed
/// app can reopen the folder at the next launch; access stays open from
/// resolution until ``stopAccessing(_:)``. Tests pass `usesSecurityScope:
/// false`: a scoped bookmark goes through ScopedBookmarkAgent and the login
/// keychain, which can block a headless CI runner.
public final class FolderGrants: @unchecked Sendable {
	private let lock = NSLock()
	private var accessed: Set<URL> = []
	private let usesSecurityScope: Bool

	public init(usesSecurityScope: Bool = true) {
		self.usesSecurityScope = usesSecurityScope
	}

	/// A bookmark for a folder the user just picked (inside the open panel's grant).
	public func makeBookmark(for folder: URL) throws -> Data {
		try folder.bookmarkData(options: usesSecurityScope ? [.withSecurityScope] : [], includingResourceValuesForKeys: nil, relativeTo: nil)
	}

	/// Resolves a stored bookmark and opens access to its folder.
	public func resolve(_ bookmark: Data) -> GrantResolution {
		var isStale = false
		let url: URL
		do {
			url = try URL(resolvingBookmarkData: bookmark, options: usesSecurityScope ? [.withSecurityScope] : [], relativeTo: nil, bookmarkDataIsStale: &isStale)
		} catch {
			return .unresolvable(reason: (error as NSError).localizedDescription)
		}
		guard startAccessing(url) else {
			return .unresolvable(reason: "Vaultday may no longer open \(url.path); grant the folder again")
		}
		guard isStale else { return .resolved(url: url, refreshedBookmark: nil) }
		// A stale bookmark still resolves; a new one is made while access is open.
		return .resolved(url: url, refreshedBookmark: try? makeBookmark(for: url))
	}

	/// Opens access to a folder the user just picked, so it can be read before the next launch.
	@discardableResult
	public func startAccessing(_ folder: URL) -> Bool {
		guard folder.startAccessingSecurityScopedResource() else {
			// Outside the sandbox there is no scope to open, and access is not limited.
			return !Self.isSandboxed
		}
		lock.withLock { _ = accessed.insert(folder) }
		return true
	}

	/// Closes access to a folder.
	public func stopAccessing(_ folder: URL) {
		let wasAccessed = lock.withLock { accessed.remove(folder) != nil }
		if wasAccessed {
			folder.stopAccessingSecurityScopedResource()
		}
	}

	/// Whether this process runs in the App Sandbox.
	public static var isSandboxed: Bool {
		ProcessInfo.processInfo.environment["APP_SANDBOX_CONTAINER_ID"] != nil
	}
}
