import Foundation
import XCTest
@testable import VaultdayCore

/// The file watcher hook: a note changed outside the app reaches the app as
/// one coalesced call, and Browse reads the vault again from it.
@MainActor
final class VaultChangeMonitorTests: XCTestCase {
	func testABurstOfBatchesIsDeliveredOnceAfterTheQuietPeriod() async throws {
		let monitor = VaultChangeMonitor(quietPeriod: .milliseconds(150))
		var calls: [[String]] = []
		monitor.onChange = { calls.append($0) }
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		XCTAssertTrue(monitor.start(workspace))
		let current = monitor.currentGeneration
		monitor.receive(["b.md"], generation: current)
		try await Task.sleep(for: .milliseconds(50))
		monitor.receive(["a.md", "b.md"], generation: current)
		monitor.receive(["stale.md"], generation: current - 1)
		try await Task.sleep(for: .milliseconds(400))
		XCTAssertEqual(calls, [["a.md", "b.md"]], "one call, after the quiet period, without the stale batch")
		monitor.stop()
		monitor.receive(["late.md"], generation: current)
		try await Task.sleep(for: .milliseconds(300))
		XCTAssertEqual(calls.count, 1, "nothing after stop")
		XCTAssertFalse(monitor.isWatching)
	}

	func testANoteEditedOutsideTheAppRefreshesBrowse() async throws {
		let harness = try OnboardingHarness(self)
		let workspace = try openWorkspace(harness)
		let browse = BrowseModel()
		browse.load(from: workspace)
		let before = browse.taskCount
		let monitor = VaultChangeMonitor(quietPeriod: .milliseconds(200))
		let delivered = expectation(description: "the change arrives")
		var paths: [String] = []
		monitor.onChange = { changed in
			paths = changed
			BrowseModel.refresh([browse])
			delivered.fulfill()
		}
		XCTAssertTrue(monitor.start(workspace))
		try await Task.sleep(for: .milliseconds(400))
		let note = URL(fileURLWithPath: workspace.rootPath).appending(path: "Projects/Website/WEB-157 Newsletter signup/WEB-157 Newsletter signup.md")
		var text = try String(contentsOf: note, encoding: .utf8)
		text = text.replacingOccurrences(of: "## Pending Input", with: "- [ ] Typed in another editor  [id:: task-90] [created:: 2026-10-22]\n\n## Pending Input")
		try text.write(to: note, atomically: true, encoding: .utf8)
		await fulfillment(of: [delivered], timeout: 10)
		monitor.stop()
		XCTAssertTrue(paths.contains { $0.hasSuffix("WEB-157 Newsletter signup.md") }, "\(paths)")
		XCTAssertEqual(browse.taskCount, before + 1)
	}

	private func openWorkspace(_ harness: OnboardingHarness) throws -> WorkspaceModel {
		let workspace = WorkspaceModel(session: harness.session)
		workspace.grantFolder(try TestFixtures.copyOfMockVault(self))
		XCTAssertEqual(workspace.phase, .open)
		return workspace
	}
}
