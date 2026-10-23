import Foundation
import XCTest
@testable import VaultdayCore

/// Sync and backup, Accounts and Feature sets: what each stores in the app-level document.
@MainActor
final class StepModelsTests: XCTestCase {
	private func workspace(_ harness: OnboardingHarness, _ workspaces: WorkspacesModel, name: String, sphere: WorkspaceSphere) throws -> WorkspaceEntry {
		let folder = try TestFixtures.emptyFolder(self, named: name)
		return try XCTUnwrap(workspaces.addWorkspace(folder: folder, sphere: sphere, name: name))
	}

	// MARK: Sync and backup

	func testSyncStartsOnLocalPlusCloudAndStoresTheModeAndService() throws {
		let harness = try OnboardingHarness(self)
		let sync = SyncStepModel(session: harness.session, workspaces: WorkspacesModel(session: harness.session))
		XCTAssertEqual(sync.home, .localAndCloud)
		XCTAssertEqual(sync.service, .iCloudDrive)
		XCTAssertFalse(sync.isServiceChosen)
		XCTAssertTrue(sync.showsCloudService)

		sync.setHome(.local)
		XCTAssertTrue(sync.showsLocalOnlyWarning)
		XCTAssertFalse(sync.showsCloudService)
		XCTAssertEqual(SyncStepModel.localOnlyWarning, "You won't be able to use your vaults on your other devices")
		sync.setHome(.cloud)
		sync.setService(.syncthing)
		sync.confirmService()

		let relaunched = SyncStepModel(session: try harness.relaunched().session, workspaces: WorkspacesModel(session: harness.session))
		XCTAssertEqual(relaunched.home, .cloud)
		XCTAssertEqual(relaunched.service, .syncthing)
		XCTAssertTrue(relaunched.isServiceChosen)
	}

	func testPickingAnotherServiceUndoesTheConfirmation() throws {
		let harness = try OnboardingHarness(self)
		let sync = SyncStepModel(session: harness.session, workspaces: WorkspacesModel(session: harness.session))
		sync.confirmService()
		XCTAssertTrue(sync.isServiceChosen)
		sync.setService(.dropbox)
		XCTAssertFalse(sync.isServiceChosen)
	}

	func testTheStepExplainsThatTheSyncFolderMustSync() {
		XCTAssertTrue(SyncStepModel.syncFolderNote.contains(".vaultday/sync"))
		XCTAssertTrue(SyncStepModel.syncFolderNote.contains("other devices"))
		XCTAssertFalse(SyncStepModel.syncFolderNote.hasSuffix("."))
	}

	func testTheGitignoreLineIsCopiedAndAppendedOnceToVaultsInGit() throws {
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let repository = try workspace(harness, workspaces, name: "InGit", sphere: .work)
		_ = try workspace(harness, workspaces, name: "NoGit", sphere: .life)
		try FileManager.default.createDirectory(at: URL(filePath: repository.rootPath).appending(path: ".git"), withIntermediateDirectories: true)
		var copied: [String] = []
		let sync = SyncStepModel(session: harness.session, workspaces: workspaces, copyToPasteboard: { copied.append($0) })

		XCTAssertEqual(sync.gitWorkspaces.map(\.workspaceName), ["InGit"])
		XCTAssertEqual(sync.gitWorkspaces.map(\.hasLine), [false])
		sync.copyGitignoreLine()
		XCTAssertEqual(copied, [sync.gitignoreLine])
		XCTAssertTrue(sync.gitignoreLine.contains(".vaultday/sync/**/*.tmp"))

		let ignore = URL(filePath: repository.rootPath).appending(path: ".gitignore")
		try Data("node_modules".utf8).write(to: ignore)
		sync.appendGitignoreLine(forWorkspace: repository.id)
		let text = try String(contentsOf: ignore, encoding: .utf8)
		XCTAssertTrue(text.hasPrefix("node_modules\n\n"), text)
		XCTAssertTrue(text.hasSuffix(sync.gitignoreLine), text)
		XCTAssertEqual(sync.gitWorkspaces.map(\.hasLine), [true])

		sync.appendGitignoreLine(forWorkspace: repository.id)
		XCTAssertEqual(try String(contentsOf: ignore, encoding: .utf8), text, "appending twice changes nothing")
		XCTAssertEqual(sync.gitMessage, ".gitignore already has the line")
	}

	func testAppendingCreatesTheGitignoreWhenThereIsNone() throws {
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let repository = try workspace(harness, workspaces, name: "Fresh", sphere: .work)
		try FileManager.default.createDirectory(at: URL(filePath: repository.rootPath).appending(path: ".git"), withIntermediateDirectories: true)
		let sync = SyncStepModel(session: harness.session, workspaces: workspaces)
		sync.appendGitignoreLine(forWorkspace: repository.id)
		let text = try String(contentsOf: URL(filePath: repository.rootPath).appending(path: ".gitignore"), encoding: .utf8)
		XCTAssertEqual(text, sync.gitignoreLine)
	}

	// MARK: Accounts

	func testAccountsAreInertAndStoreWhatEachWorkspaceWants() throws {
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let work = try workspace(harness, workspaces, name: "Work", sphere: .work)
		let life = try workspace(harness, workspaces, name: "Life", sphere: .life)
		let accounts = AccountsStepModel(session: harness.session)
		accounts.refresh(workspaces: workspaces.entries)
		XCTAssertFalse(accounts.canAddAccounts)
		XCTAssertFalse(AccountsStepModel.plannedNote.hasSuffix("."))
		XCTAssertFalse(accounts.isOn(.mail, workspace: work.id))

		accounts.set(.mail, isOn: true, workspace: work.id)
		accounts.set(.reminders, isOn: true, workspace: life.id)
		accounts.set(.mail, isOn: true, workspace: life.id)
		accounts.set(.mail, isOn: false, workspace: life.id)
		XCTAssertTrue(accounts.isOn(.mail, workspace: work.id))
		XCTAssertFalse(accounts.isOn(.mail, workspace: life.id))

		let relaunched = AccountsStepModel(session: try harness.relaunched().session)
		relaunched.refresh(workspaces: workspaces.entries)
		XCTAssertTrue(relaunched.isOn(.mail, workspace: work.id))
		XCTAssertTrue(relaunched.isOn(.reminders, workspace: life.id))
		XCTAssertFalse(relaunched.isOn(.mail, workspace: life.id))
		XCTAssertEqual(try harness.session.appState.connections().filter { $0.kind == .mailAccount }.count, 0, "no account is created")
	}

	// MARK: Feature sets

	func testEveryTileIsOffAndPlannedWithTheBoardsFiveSets() throws {
		let harness = try OnboardingHarness(self)
		let features = FeatureSetsStepModel(session: harness.session)
		XCTAssertEqual(features.tiles.map(\.title), ["GitHub", "ServiceNow", "Azure DevOps", "Teams and work mail", "Toggl and billing"])
		XCTAssertTrue(features.tiles.allSatisfy { $0.scope == .off })
		XCTAssertEqual(features.tiles.filter(\.raisesEgress).map(\.title), ["GitHub", "Toggl and billing"])
		XCTAssertTrue(features.tiles.allSatisfy { !$0.title.contains("Outlook") && !$0.description.hasSuffix(".") })
		XCTAssertEqual(FeatureScopeSwitch.allCases.map(\.title), ["Off", "Everywhere", "Per sphere", "Per workspace"])
	}

	func testTheScopeSwitchIsStoredThroughTheRegistryAndSurvivesARelaunch() throws {
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let work = try workspace(harness, workspaces, name: "Work", sphere: .work)
		let life = try workspace(harness, workspaces, name: "Life", sphere: .life)
		let features = FeatureSetsStepModel(session: harness.session)
		features.refresh(workspaces: workspaces.entries)

		features.setScope(.perSphere, tile: "servicenow")
		XCTAssertEqual(try XCTUnwrap(features.tiles.first { $0.id == "servicenow" }).targets, ["work", "life"])
		features.setTarget("life", isOn: false, tile: "servicenow")
		features.setScope(.perWorkspace, tile: "github")
		features.setTarget(life.id, isOn: false, tile: "github")

		let stored = harness.session.appState.featureScope(feature: "servicenow")
		XCTAssertEqual(stored.mode, .perSphere)
		XCTAssertEqual(stored.targets, ["work"])
		let github = harness.session.appState.featureScope(feature: "github")
		XCTAssertEqual(github.mode, .perWorkspace)
		XCTAssertEqual(github.targets, [work.id])

		let relaunched = FeatureSetsStepModel(session: try harness.relaunched().session)
		XCTAssertEqual(try XCTUnwrap(relaunched.tiles.first { $0.id == "servicenow" }).scope, .perSphere)
		XCTAssertEqual(try XCTUnwrap(relaunched.tiles.first { $0.id == "github" }).targets, [work.id])

		features.setScope(.off, tile: "servicenow")
		XCTAssertEqual(harness.session.appState.featureScope(feature: "servicenow").mode, .off)
	}

	func testAWorkspaceAddedLaterIsCoveredByEverywhere() throws {
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		_ = try workspace(harness, workspaces, name: "First", sphere: .work)
		let features = FeatureSetsStepModel(session: harness.session)
		features.refresh(workspaces: workspaces.entries)
		features.setScope(.everywhere, tile: "toggl-billing")
		let second = try workspace(harness, workspaces, name: "Second", sphere: .life)
		features.refresh(workspaces: workspaces.entries)
		XCTAssertEqual(features.tiles.first { $0.id == "toggl-billing" }?.scope, .everywhere)
		XCTAssertNotNil(second)
	}
}
