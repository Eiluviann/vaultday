import AppKit
import SwiftUI
import VaultdayCore
import XCTest

/// The setup window's proportions from the Onboarding boards: the 240 px
/// step rail at every window width and the 860 × 680 Configure sheet.
@MainActor
final class SetupLayoutTests: XCTestCase {
	private static let mockVault = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.appendingPathComponent("core/fixtures/mock-vault")

	private func onboarding() throws -> OnboardingModel {
		let container = FileManager.default.temporaryDirectory.appending(path: "vaultday-layout-\(UUID().uuidString)", directoryHint: .isDirectory)
		addTeardownBlock { try? FileManager.default.removeItem(at: container) }
		let session = try CoreSession.open(try CoreEnvironment.inMemory(container: container))
		return OnboardingModel(session: session, workspaces: WorkspacesModel(session: session))
	}

	func testTheBoardsDimensionsAreTheTokens() {
		XCTAssertEqual(SetupLayout.sidebarWidth, 240)
		XCTAssertEqual(SetupLayout.titleBarHeight, 38)
		XCTAssertEqual(SetupLayout.gutter, 32)
		XCTAssertEqual(SetupLayout.measure, 640)
		XCTAssertEqual(SetupLayout.configureSheetSize, CGSize(width: 860, height: 680))
		XCTAssertEqual(SetupLayout.windowDefaultSize, CGSize(width: 1100, height: 800))
	}

	func testTheStepRailStaysAt240PointsHoweverMuchRoomItIsOffered() throws {
		let model = try onboarding()
		let controller = NSHostingController(rootView: SetupSidebar(onboarding: model, scheme: .constant(.vimMotions)))
		for offered: CGFloat in [200, 240, 600, 1440] {
			let size = controller.sizeThatFits(in: CGSize(width: offered, height: 800))
			XCTAssertEqual(size.width, 240, accuracy: 0.5, "offered \(offered) points")
		}
		let row = NSHostingController(rootView: HStack(spacing: 0) {
			SetupSidebar(onboarding: model, scheme: .constant(.standard))
			Color.clear.frame(width: 560)
		}.frame(height: 800))
		XCTAssertEqual(row.sizeThatFits(in: CGSize(width: 4000, height: 800)).width, 240 + 560, accuracy: 0.5, "the rail and a 560 pane make the window's minimum")
	}

	func testTheConfigureSheetIs860By680() throws {
		let model = WorkspaceConfigureModel(root: Self.mockVault.path)
		XCTAssertTrue(model.isLoaded)
		let host = NSHostingView(rootView: ConfigureSheet(model: model, onDone: {}))
		XCTAssertEqual(host.fittingSize.width, 860, accuracy: 0.5)
		XCTAssertEqual(host.fittingSize.height, 680, accuracy: 0.5)
	}

	func testTheStepListNamesAllTwelveStepsInOrder() throws {
		XCTAssertEqual(SetupStep.allCases.map(\.title), [
			"Welcome", "Workspaces", "Folders", "Sync and backup", "Accounts", "Feature sets",
			"Security", "Rituals and routines", "AI level", "AI providers", "AI assistance", "Ready",
		])
	}
}
