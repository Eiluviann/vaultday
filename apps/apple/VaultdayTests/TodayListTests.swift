import XCTest
import VaultdayCore

/// The app's own test bundle: the fixture workspace loads through the package
/// the app links, with the same values the window lists.
final class TodayListTests: XCTestCase {
	/// `apps/apple/VaultdayTests/<file>` → repo root.
	private static let mockVault = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.appendingPathComponent("core/fixtures/mock-vault")
		.path

	@MainActor
	func testTheFixtureWorkspaceListsItsOpenWork() {
		let model = DayPlanModel()
		model.load(workspacePath: Self.mockVault, date: "2026-10-22")

		XCTAssertNil(model.errorMessage)
		XCTAssertEqual(model.openTaskCount, 9)
		XCTAssertEqual(model.log.count, 4)
	}
}
