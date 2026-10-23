import XCTest
@testable import VaultdayCore

final class DayPlanModelTests: XCTestCase {
	private static let repoRoot = TestFixtures.repoRoot
	private static let mockVault = TestFixtures.mockVault.path

	@MainActor
	func testTheMockVaultLoadsIntoPlainValues() {
		let model = DayPlanModel()
		model.load(workspacePath: Self.mockVault, date: "2026-10-22")

		XCTAssertNil(model.errorMessage)
		XCTAssertEqual(model.workspaceName, "Mock Studio")
		XCTAssertEqual(model.stories.map(\.number), ["WEB-142", "WEB-157", "ACME-9"])
		XCTAssertEqual(model.stories[0].title, "Checkout redesign")
		XCTAssertEqual(model.stories[0].nextSteps.count, 4)
		XCTAssertTrue(model.stories[0].nextSteps[2].isInProgress)
		XCTAssertEqual(model.stories[0].pendingInput.map(\.assignedTo), ["Dana"])
		XCTAssertEqual(model.log.map(\.time), ["08:45", "09:30", "11:15", "14:00"])
		XCTAssertNotNil(model.lastCallMilliseconds)
	}

	@MainActor
	func testAFolderWithoutAManifestReportsAnError() {
		let model = DayPlanModel()
		model.load(workspacePath: Self.repoRoot.path)

		XCTAssertEqual(model.stories, [])
		XCTAssertTrue(model.errorMessage?.contains("not a workspace") == true, model.errorMessage ?? "nil")
	}
}
