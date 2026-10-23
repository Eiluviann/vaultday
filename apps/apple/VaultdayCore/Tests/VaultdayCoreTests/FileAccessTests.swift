import Foundation
import XCTest
@testable import VaultdayCore

/// Security-scoped bookmarks round-trip in a plain test process too (no
/// sandbox needed to make or resolve one). They go through ScopedBookmarkAgent
/// and the login keychain, which can block a headless CI runner, so the
/// bookmark tests run on a Mac only.
final class FileAccessTests: XCTestCase {
	private func skipOnCI() throws {
		try XCTSkipIf(ProcessInfo.processInfo.environment["CI"] != nil, "security-scoped bookmarks need ScopedBookmarkAgent and a login keychain")
	}

	func testABookmarkRoundTripsToTheSameFolder() throws {
		try skipOnCI()
		let folder = try TestFixtures.temporaryFolder(self, "granted")
		let grants = FolderGrants()
		let bookmark = try grants.makeBookmark(for: folder)

		guard case let .resolved(url, refreshed) = grants.resolve(bookmark) else {
			return XCTFail("the bookmark did not resolve")
		}
		XCTAssertEqual(url.standardizedFileURL.path, folder.standardizedFileURL.path)
		XCTAssertNil(refreshed, "a fresh bookmark is not stale")
		grants.stopAccessing(url)
	}

	func testAMovedFolderStillResolvesAndAStaleBookmarkIsRefreshed() throws {
		try skipOnCI()
		let parent = try TestFixtures.temporaryFolder(self, "parent")
		let folder = parent.appending(path: "Vault", directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
		let grants = FolderGrants()
		let bookmark = try grants.makeBookmark(for: folder)
		let moved = parent.appending(path: "Renamed vault", directoryHint: .isDirectory)
		try FileManager.default.moveItem(at: folder, to: moved)

		guard case let .resolved(url, refreshed) = grants.resolve(bookmark) else {
			return XCTFail("a moved folder should still resolve")
		}
		XCTAssertEqual(url.lastPathComponent, "Renamed vault")
		if let refreshed {
			guard case let .resolved(again, _) = grants.resolve(refreshed) else { return XCTFail("refreshed bookmark") }
			XCTAssertEqual(again.lastPathComponent, "Renamed vault")
		}
	}

	func testABookmarkToADeletedFolderOrGarbageIsUnresolvable() throws {
		try skipOnCI()
		let folder = try TestFixtures.temporaryFolder(self, "gone")
		let grants = FolderGrants()
		let bookmark = try grants.makeBookmark(for: folder)
		try FileManager.default.removeItem(at: folder)

		guard case .unresolvable = grants.resolve(bookmark) else { return XCTFail("a deleted folder resolved") }
		guard case .unresolvable = grants.resolve(Data("not a bookmark".utf8)) else { return XCTFail("garbage resolved") }
	}

	func testTheFilePortRefusesPathsOutsideItsGrants() throws {
		let granted = try TestFixtures.temporaryFolder(self, "granted")
		let elsewhere = try TestFixtures.temporaryFolder(self, "elsewhere")
		let files = CoordinatedSyncFiles(roots: [granted])

		try files.writeAtomic(path: granted.appending(path: ".vaultday/sync/dev/0001.bin").path, bytes: Data([1, 2]))
		XCTAssertEqual(try files.read(path: granted.appending(path: ".vaultday/sync/dev/0001.bin").path), Data([1, 2]))
		XCTAssertThrowsError(try files.writeAtomic(path: elsewhere.appending(path: "x.bin").path, bytes: Data()))
		XCTAssertThrowsError(try files.read(path: granted.path + "-sibling/x.bin"), "a sibling with the same prefix is outside")
		XCTAssertThrowsError(try files.read(path: granted.appending(path: "../elsewhere/x.bin").path))

		files.allow(elsewhere)
		try files.writeAtomic(path: elsewhere.appending(path: "x.bin").path, bytes: Data([3]))
	}

	func testMissingFilesAndFoldersKeepTheCoreContract() throws {
		let granted = try TestFixtures.temporaryFolder(self, "granted")
		let files = CoordinatedSyncFiles(roots: [granted])
		XCTAssertEqual(try files.list(dir: granted.appending(path: "none").path), [])
		XCTAssertThrowsError(try files.read(path: granted.appending(path: "none.bin").path)) {
			guard case .NotFound = $0 as? FileFailure else { return XCTFail("\($0)") }
		}
		try files.remove(path: granted.appending(path: "none.bin").path)
	}

	func testListingShowsPlaceholdersAndADownloadIsRequestedForTheRealFile() throws {
		let granted = try TestFixtures.temporaryFolder(self, "granted")
		let device = granted.appending(path: "dev", directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: device, withIntermediateDirectories: true)
		FileManager.default.createFile(atPath: device.appending(path: ".0003.bin.icloud").path, contents: Data())
		let downloader = RecordingDownloader()
		let files = CoordinatedSyncFiles(roots: [granted], downloader: downloader)

		let names = try files.list(dir: device.path).map { URL(fileURLWithPath: $0.path).lastPathComponent }
		XCTAssertEqual(names, [".0003.bin.icloud"], "hidden placeholders are listed")
		try files.requestDownload(placeholder: device.appending(path: ".0003.bin.icloud").path)
		XCTAssertEqual(downloader.requested.map(\.lastPathComponent), ["0003.bin"])
		XCTAssertThrowsError(try files.requestDownload(placeholder: device.appending(path: "0003.bin").path))
	}

	func testAReadOfAnEvictedFileStartsItsDownloadAndDoesNotWaitForIt() throws {
		let granted = try TestFixtures.temporaryFolder(self, "granted")
		let file = granted.appending(path: "0001.bin")
		try Data([7]).write(to: file)
		let downloader = RecordingDownloader(evicted: [file.standardizedFileURL])
		let files = CoordinatedSyncFiles(roots: [granted], downloader: downloader)

		XCTAssertThrowsError(try files.read(path: file.path)) {
			guard case let .Failed(message) = $0 as? FileFailure else { return XCTFail("\($0)") }
			XCTAssertTrue(message.contains("downloading"), message)
		}
		XCTAssertEqual(downloader.requested.map(\.lastPathComponent), ["0001.bin"])
	}

	func testASymlinkOutOfAGrantedFolderIsRefused() throws {
		let granted = try TestFixtures.temporaryFolder(self, "granted")
		let elsewhere = try TestFixtures.temporaryFolder(self, "elsewhere")
		try Data([1]).write(to: elsewhere.appending(path: "secret.bin"))
		try FileManager.default.createSymbolicLink(at: granted.appending(path: "link"), withDestinationURL: elsewhere)
		let files = CoordinatedSyncFiles(roots: [granted])

		XCTAssertThrowsError(try files.read(path: granted.appending(path: "link/secret.bin").path))
		XCTAssertThrowsError(try files.remove(path: granted.appending(path: "link/secret.bin").path))
		XCTAssertTrue(FileManager.default.fileExists(atPath: elsewhere.appending(path: "secret.bin").path))
	}

	func testTheVolatileContainerMayNotSitInsideAWorkspace() throws {
		let workspace = try TestFixtures.temporaryFolder(self, "workspace")
		XCTAssertTrue(AppGroupLayout(container: workspace.appending(path: "Group")).isInside(workspace))
		XCTAssertFalse(AppGroupLayout(container: workspace.deletingLastPathComponent().appending(path: "Group")).isInside(workspace))
	}
}

private final class RecordingDownloader: UbiquitousDownloader, @unchecked Sendable {
	private let lock = NSLock()
	private var urls: [URL] = []
	private let evicted: Set<URL>

	init(evicted: Set<URL> = []) {
		self.evicted = evicted
	}

	var requested: [URL] { lock.withLock { urls } }

	func needsDownload(_ url: URL) -> Bool {
		evicted.contains(url.standardizedFileURL) || evicted.contains(url.standardizedFileURL.resolvingSymlinksInPath())
	}

	func startDownloading(_ url: URL) throws {
		lock.withLock { urls.append(url) }
	}
}
