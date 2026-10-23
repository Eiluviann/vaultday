import AppKit
import SwiftUI
import VaultdayCore
import XCTest

/// Renders every drawn step and the Configure sheet to PNG, for a review
/// beside the boards. Runs only when `VAULTDAY_SNAPSHOT_DIR` names a folder.
@MainActor
final class SetupSnapshotTests: XCTestCase {
	private static let mockVault = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.deletingLastPathComponent()
		.appendingPathComponent("core/fixtures/mock-vault")

	override func setUp() async throws {
		// Measure and draw with the faces the app ships, not the system fallback.
		DesignFonts.registerBundledFonts(in: Bundle(for: SetupSnapshotTests.self))
	}

	func testRenderEveryStepAndTheConfigureSheet() throws {
		guard let path = ProcessInfo.processInfo.environment["VAULTDAY_SNAPSHOT_DIR"] else {
			throw XCTSkip("set VAULTDAY_SNAPSHOT_DIR to write the snapshots")
		}
		let output = URL(fileURLWithPath: path, isDirectory: true)
		try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

		let scratch = FileManager.default.temporaryDirectory.appending(path: "vaultday-snapshots-\(UUID().uuidString)", directoryHint: .isDirectory)
		addTeardownBlock { try? FileManager.default.removeItem(at: scratch) }
		let session = try CoreSession.open(try CoreEnvironment.inMemory(container: scratch.appending(path: "Group", directoryHint: .isDirectory)))
		let workspaces = WorkspacesModel(session: session)
		for (name, sphere) in [("Freelancing", WorkspaceSphere.work), ("Client XYZ", .work), ("Personal", .life)] {
			let folder = scratch.appending(path: name, directoryHint: .isDirectory)
			try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
			workspaces.addWorkspace(folder: folder, sphere: sphere, name: name)
		}
		let docs = scratch.appending(path: "docs", directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: docs, withIntermediateDirectories: true)
		if let first = workspaces.entries.first { workspaces.addFolder(docs, to: first.id) }
		let onboarding = OnboardingModel(session: session, workspaces: workspaces)
		let sync = SyncStepModel(session: session, workspaces: workspaces)
		let accounts = AccountsStepModel(session: session)
		accounts.refresh(workspaces: workspaces.entries)
		let features = FeatureSetsStepModel(session: session)
		features.refresh(workspaces: workspaces.entries)
		features.setScope(.perWorkspace, tile: "github")
		let security = SecurityStepModel(session: session)
		security.refresh(workspaces: workspaces.entries)
		let rhythm = RhythmModel(session: session)
		let aiLevel = AiLevelStepModel(session: session)
		aiLevel.refresh(workspaces: workspaces.entries)
		aiLevel.setMode(.perWorkspace)
		let aiProviders = AiProvidersStepModel(session: session)
		var draft = ProviderKeyDraft(provider: .claude)
		draft.name = "Work key"
		draft.key = "sk-ant-synthetic-snapshot-key"
		aiProviders.add(draft)
		let aiAssistance = AiAssistanceStepModel(session: session)
		aiAssistance.refresh(workspaces: workspaces.entries)
		aiAssistance.setMode(.perSphere, feature: "sort-inbox")
		let ready = ReadyStepModel(session: session, rhythm: rhythm)
		ready.refresh(workspaces: workspaces.entries)

		for step in SetupStep.allCases {
			while onboarding.current != step { onboarding.goForward() }
			let frame = SetupFrame(onboarding: onboarding, scheme: .constant(.vimMotions), chooseRestoreFolder: { _ in }, finish: {}) {
				switch step {
				case .welcome: WelcomeStep()
				case .workspaces: WorkspacesStep(workspaces: workspaces)
				case .folders: FoldersStep(workspaces: workspaces)
				case .syncAndBackup: SyncStep(sync: sync)
				case .accounts: AccountsStep(workspaces: workspaces, accounts: accounts)
				case .featureSets: FeatureSetsStep(workspaces: workspaces, features: features)
				case .security: SecurityStep(workspaces: workspaces, security: security)
				case .rituals: RitualsStep(rhythm: rhythm)
				case .aiLevel: AiLevelStep(workspaces: workspaces, level: aiLevel)
				case .aiProviders: AiProvidersStep(workspaces: workspaces, providers: aiProviders)
				case .aiAssistance: AiAssistanceStep(workspaces: workspaces, assist: aiAssistance)
				case .ready: ReadyStep(workspaces: workspaces, ready: ready)
				}
			}
			.frame(width: SetupLayout.boardSize.width, height: SetupLayout.boardSize.height - SetupLayout.titleBarHeight)
			try write(frame, to: output.appending(path: "step-\(step.rawValue)-\(step.title.lowercased().replacingOccurrences(of: " ", with: "-")).png"))
		}
		let sheet = ConfigureSheet(model: WorkspaceConfigureModel(root: Self.mockVault.path), onDone: {})
		try write(sheet, to: output.appending(path: "configure-sheet.png"))
	}

	private func write(_ view: some View, to file: URL) throws {
		let host = NSHostingView(rootView: view.environment(\.colorScheme, .light))
		host.frame = NSRect(origin: .zero, size: host.fittingSize)
		host.layoutSubtreeIfNeeded()
		let rep = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
		host.cacheDisplay(in: host.bounds, to: rep)
		let png = try XCTUnwrap(rep.representation(using: .png, properties: [:]))
		try png.write(to: file)
	}
}
