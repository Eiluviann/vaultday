import Foundation

/// Starts the download of a file the sync provider evicted.
public protocol UbiquitousDownloader: Sendable {
	/// Whether `url` is a cloud file that is not on disk in its current version.
	func needsDownload(_ url: URL) -> Bool
	/// Asks the provider to download `url`.
	func startDownloading(_ url: URL) throws
}

/// The core's byte-level file port for app state, as the PR 4 rules ask on
/// Apple: every read and write is coordinated with `NSFileCoordinator`, so
/// iCloud Drive and other file providers see a consistent file; a read of an
/// evicted file starts its download and answers "not arrived yet" (the core
/// retries at its next pull) rather than block the caller until the download
/// ends; writes are atomic; and nothing outside the granted folders and the
/// App Group container is touched, symlinks resolved.
public final class CoordinatedSyncFiles: SyncFilesPort, @unchecked Sendable {
	private let lock = NSLock()
	private var roots: [URL]
	private let downloader: any UbiquitousDownloader

	/// - Parameters:
	///   - roots: folders the port may touch (the App Group container); granted
	///     workspace folders are added with ``allow(_:)``.
	public init(roots: [URL], downloader: any UbiquitousDownloader = FileManagerDownloader()) {
		self.roots = roots.map(Self.resolved)
		self.downloader = downloader
	}

	/// Lets the port touch `folder` and everything below it.
	public func allow(_ folder: URL) {
		lock.withLock { roots.append(Self.resolved(folder)) }
	}

	/// Stops the port touching `folder`.
	public func disallow(_ folder: URL) {
		let folder = Self.resolved(folder)
		lock.withLock { roots.removeAll { $0 == folder } }
	}

	// MARK: SyncFilesPort

	public func list(dir: String) throws -> [SyncEntry] {
		let url = try allowedURL(dir)
		var entries: [SyncEntry] = []
		var failure: Error?
		coordinate(reading: url) { url in
			do {
				let names = try FileManager.default.contentsOfDirectory(atPath: url.path)
				entries = names.compactMap { name in
					let child = url.appending(path: name)
					var isDirectory: ObjCBool = false
					// An entry that vanished since the listing is not an error for the folder.
					guard FileManager.default.fileExists(atPath: child.path, isDirectory: &isDirectory) else { return nil }
					return SyncEntry(path: child.path, isDir: isDirectory.boolValue)
				}
			} catch let error as CocoaError where error.code == .fileReadNoSuchFile {
				entries = []
			} catch {
				failure = error
			}
		} failed: { failure = $0 }
		if let failure { throw fileFailure(failure, path: dir) }
		return entries
	}

	public func read(path: String) throws -> Data {
		let url = try allowedURL(path)
		if downloader.needsDownload(url) {
			do {
				try downloader.startDownloading(url)
			} catch {
				throw fileFailure(error, path: path)
			}
			throw FileFailure.Failed(message: "\(path) is downloading from iCloud")
		}
		var data: Data?
		var failure: Error?
		coordinate(reading: url) { url in
			do {
				data = try Data(contentsOf: url)
			} catch {
				failure = error
			}
		} failed: { failure = $0 }
		if let failure { throw fileFailure(failure, path: path) }
		guard let data else { throw FileFailure.Failed(message: "\(path) could not be read") }
		return data
	}

	public func writeAtomic(path: String, bytes: Data) throws {
		let url = try allowedURL(path)
		var failure: Error?
		coordinate(writing: url, options: .forReplacing) { url in
			do {
				try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
				try bytes.write(to: url, options: [.atomic])
			} catch {
				failure = error
			}
		} failed: { failure = $0 }
		if let failure { throw fileFailure(failure, path: path) }
	}

	public func remove(path: String) throws {
		let url = try allowedURL(path)
		var failure: Error?
		coordinate(writing: url, options: .forDeleting) { url in
			do {
				try FileManager.default.removeItem(at: url)
			} catch let error as CocoaError where error.code == .fileNoSuchFile {
				return
			} catch {
				failure = error
			}
		} failed: { failure = $0 }
		if let failure { throw fileFailure(failure, path: path) }
	}

	public func requestDownload(placeholder: String) throws {
		let placeholderURL = try allowedURL(placeholder)
		guard let target = Self.fileForPlaceholder(placeholderURL) else {
			throw FileFailure.Failed(message: "\(placeholder) is not an iCloud placeholder")
		}
		do {
			try downloader.startDownloading(target)
		} catch {
			throw fileFailure(error, path: placeholder)
		}
	}

	// MARK: Helpers

	/// The file an iCloud placeholder stands for: `.0003.bin.icloud` → `0003.bin`.
	static func fileForPlaceholder(_ placeholder: URL) -> URL? {
		let name = placeholder.lastPathComponent
		guard name.hasPrefix("."), name.hasSuffix(".icloud"), name.count > 1 + ".icloud".count else { return nil }
		let original = String(name.dropFirst().dropLast(".icloud".count))
		return placeholder.deletingLastPathComponent().appending(path: original)
	}

	private func allowedURL(_ path: String) throws -> URL {
		let url = URL(fileURLWithPath: path).standardizedFileURL
		// Checked on the real path, so a symlink inside a granted folder that
		// points elsewhere is refused.
		let real = Self.resolved(url)
		let isAllowed = lock.withLock {
			roots.contains { root in
				real.path == root.path || real.path.hasPrefix(root.path.hasSuffix("/") ? root.path : root.path + "/")
			}
		}
		guard isAllowed else {
			throw FileFailure.Failed(message: "\(path) is outside every folder Vaultday was granted")
		}
		return url
	}

	/// The path with every symlink in its existing part resolved.
	private static func resolved(_ url: URL) -> URL {
		url.standardizedFileURL.resolvingSymlinksInPath()
	}

	private func coordinate(reading url: URL, _ body: (URL) -> Void, failed: (Error) -> Void) {
		var error: NSError?
		NSFileCoordinator(filePresenter: nil).coordinate(readingItemAt: url, options: [], error: &error, byAccessor: body)
		if let error { failed(error) }
	}

	private func coordinate(writing url: URL, options: NSFileCoordinator.WritingOptions, _ body: (URL) -> Void, failed: (Error) -> Void) {
		var error: NSError?
		NSFileCoordinator(filePresenter: nil).coordinate(writingItemAt: url, options: options, error: &error, byAccessor: body)
		if let error { failed(error) }
	}

	private func fileFailure(_ error: Error, path: String) -> FileFailure {
		let nsError = error as NSError
		let isNotFound = (nsError.domain == NSCocoaErrorDomain
			&& [NSFileReadNoSuchFileError, NSFileNoSuchFileError].contains(nsError.code))
			|| (nsError.domain == NSPOSIXErrorDomain && nsError.code == Int(ENOENT))
		let message = "\(path): \(nsError.localizedDescription)"
		return isNotFound ? .NotFound(message: message) : .Failed(message: message)
	}
}

/// ``UbiquitousDownloader`` over `FileManager`.
public struct FileManagerDownloader: UbiquitousDownloader {
	public init() {}

	public func needsDownload(_ url: URL) -> Bool {
		guard FileManager.default.isUbiquitousItem(at: url),
		      let values = try? url.resourceValues(forKeys: [.ubiquitousItemDownloadingStatusKey]),
		      let status = values.ubiquitousItemDownloadingStatus
		else { return false }
		return status != .current
	}

	public func startDownloading(_ url: URL) throws {
		try FileManager.default.startDownloadingUbiquitousItem(at: url)
	}
}
