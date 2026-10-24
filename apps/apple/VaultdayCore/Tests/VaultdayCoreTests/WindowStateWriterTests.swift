import XCTest
@testable import VaultdayCore

/// The window state is written on the key path, so a key that changes
/// nothing it holds (`j`, `k`) must not touch the disk.
@MainActor
final class WindowStateWriterTests: XCTestCase {
	func testOnlyAChangedStateIsWritten() throws {
		let store = WindowStateStore(layout: AppGroupLayout(container: try TestFixtures.temporaryFolder(self, "Group")))
		let writer = WindowStateWriter(store: store, loaded: WindowState())
		XCTAssertFalse(writer.save(WindowState()), "the state just loaded is already on disk")
		XCTAssertTrue(writer.save(WindowState(focusedPane: .inbox)))
		XCTAssertEqual(store.load(), WindowState(focusedPane: .inbox))
		for _ in 0 ..< 100 {
			XCTAssertFalse(writer.save(WindowState(focusedPane: .inbox)), "a move inside the pane writes nothing")
		}
		XCTAssertTrue(writer.save(WindowState(isSidebarCollapsed: true, focusedPane: .inbox)))
		XCTAssertEqual(store.load(), WindowState(isSidebarCollapsed: true, focusedPane: .inbox))
	}
}
