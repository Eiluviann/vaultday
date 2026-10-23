import Foundation
import XCTest
@testable import VaultdayCore

/// AI level, AI providers and AI assistance: stored through the core, inert, and no key ever leaves the Keychain.
@MainActor
final class AiStepModelsTests: XCTestCase {
	private func workspaces(_ harness: OnboardingHarness) throws -> [WorkspaceEntry] {
		let model = WorkspacesModel(session: harness.session)
		let first = try XCTUnwrap(model.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Freelancing"), sphere: .work, name: "Freelancing"))
		let second = try XCTUnwrap(model.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Personal"), sphere: .life, name: "Personal"))
		return [first, second]
	}

	// MARK: AI level

	func testTheLevelStartsOffEverywhereAndKeepsWhatIsChosen() throws {
		let harness = try OnboardingHarness(self)
		let entries = try workspaces(harness)
		let level = AiLevelStepModel(session: harness.session)
		level.refresh(workspaces: entries)
		XCTAssertEqual(level.scope.mode, .off)
		XCTAssertEqual(level.level(of: entries[0].id), .off)
		XCTAssertEqual(level.retention, .zeroRetention)

		level.setMode(.perWorkspace)
		XCTAssertEqual(level.level(of: entries[0].id), .glance, "everything starts at the light level")
		level.setLevel(.insight, target: entries[0].id)
		level.setLevel(.off, target: entries[1].id)
		level.setRetention(.safetyRetention)

		let reopened = AiLevelStepModel(session: try harness.relaunched().session)
		XCTAssertEqual(reopened.scope.mode, .perWorkspace)
		XCTAssertEqual(reopened.level(of: entries[0].id), .insight)
		XCTAssertEqual(reopened.level(of: entries[1].id), .off)
		XCTAssertEqual(reopened.retention, .safetyRetention)
	}

	func testTheLevelCanBeSetEverywhereOrPerSphereAndSwitchedOff() throws {
		let harness = try OnboardingHarness(self)
		let level = AiLevelStepModel(session: harness.session)
		level.refresh(workspaces: try workspaces(harness))
		level.setMode(.everywhere)
		level.setLevel(.insight, target: AiScope.wholeScope)
		XCTAssertEqual(level.level(of: AiScope.wholeScope), .insight)
		level.setMode(.perSphere)
		XCTAssertEqual(level.scope.targets, ["work", "life"])
		level.setLevel(.insight, target: "work")
		level.setLevel(.off, target: "life")
		XCTAssertEqual(level.level(of: "work"), .insight)
		XCTAssertEqual(level.level(of: "life"), .off)
		level.setMode(.off)
		XCTAssertEqual(level.scope, AiScope())
		level.setLevel(.insight, target: "work")
		XCTAssertEqual(level.scope.mode, .off, "with the level off there is nothing to set")
	}

	// MARK: AI assistance

	func testEachFeatureKeepsItsScopeAndLevelAndTheGuardrailsPersist() throws {
		let harness = try OnboardingHarness(self)
		let entries = try workspaces(harness)
		let assist = AiAssistanceStepModel(session: harness.session)
		assist.refresh(workspaces: entries)
		XCTAssertEqual(AiFeature.all.count, 10)
		XCTAssertTrue(AiFeature.all.allSatisfy { assist.scope(of: $0.id).mode == .off })

		assist.setMode(.perSphere, feature: "sort-inbox")
		assist.setTier(.insight, feature: "sort-inbox")
		assist.setTarget("life", isOn: false, feature: "sort-inbox")
		assist.setMode(.everywhere, feature: "quick-add")
		assist.setTier(.insight, feature: "quick-add")
		assist.setMode(.perWorkspace, feature: "drafts")
		assist.setTarget(entries[1].id, isOn: false, feature: "drafts")
		assist.setClaudeCodeMode(.perSphere)
		assist.setClaudeCodeTarget("life", isOn: false)
		assist.setOtherApps(.readOnly)
		assist.setAgentChanges(.askBeforeDeleting)
		assist.setPreviewsBeforeSending(false)
		assist.setKeepsActivityLog(false)

		let reopened = AiAssistanceStepModel(session: try harness.relaunched().session)
		XCTAssertEqual(reopened.scope(of: "sort-inbox").mode, .perSphere)
		XCTAssertEqual(reopened.scope(of: "sort-inbox").targets, ["work"])
		XCTAssertEqual(reopened.scope(of: "sort-inbox").wholeTier, .insight)
		XCTAssertEqual(reopened.scope(of: "quick-add").wholeTier, .glance, "a Glance-only feature takes no other level")
		XCTAssertEqual(reopened.scope(of: "drafts").targets, [entries[0].id])
		XCTAssertEqual(reopened.claudeCode.mode, .perSphere)
		XCTAssertEqual(reopened.claudeCode.targets, ["work"])
		XCTAssertEqual(reopened.otherApps, .readOnly)
		XCTAssertEqual(reopened.agentChanges, .askBeforeDeleting)
		XCTAssertFalse(reopened.previewsBeforeSending)
		XCTAssertFalse(reopened.keepsActivityLog)
		XCTAssertEqual(reopened.scope(of: "search").mode, .off)
	}

	func testAgentsDefaultToAskingAndOtherAppsToOff() throws {
		let assist = AiAssistanceStepModel(session: try OnboardingHarness(self).session)
		XCTAssertEqual(assist.agentChanges, .askEveryTime)
		XCTAssertEqual(assist.otherApps, .off)
		XCTAssertTrue(assist.previewsBeforeSending, "a cloud model sees nothing unpreviewed until the user says so")
	}

	// MARK: AI providers

	func testAClaudeKeyGoesToTheKeychainAndNowhereElse() throws {
		let harness = try OnboardingHarness(self)
		let entries = try workspaces(harness)
		let providers = AiProvidersStepModel(session: harness.session)
		let secret = "sk-ant-api03-synthetic-\(UUID().uuidString)"
		var draft = ProviderKeyDraft(provider: .claude)
		draft.name = "Work key"
		draft.key = secret
		draft.scope = .perSphere
		draft.targets = ["work"]
		guard case let .added(id) = providers.add(draft) else { return XCTFail("the key was refused: \(providers.issues)") }

		// The Keychain store double holds it, as a device-only item.
		let item = try XCTUnwrap(harness.session.environment.secrets.get(id: SecretId(kind: .providerApiKey, name: id)))
		XCTAssertEqual(String(decoding: item.value, as: UTF8.self), secret)
		XCTAssertFalse(item.isSynchronisable)

		// The list has the record and no key: nothing printed from it holds the text.
		let entry = try XCTUnwrap(providers.keys.first)
		XCTAssertEqual(entry.name, "Work key")
		XCTAssertEqual(entry.provider, .claude)
		XCTAssertEqual(entry.scope, .perSphere)
		XCTAssertEqual(entry.targets, ["work"])
		XCTAssertTrue(entry.isKeyStored)
		XCTAssertEqual(entry.insightModel, "claude-sonnet-5-5")
		XCTAssertEqual(entry.glanceModel, "claude-haiku-5-5")
		for text in [String(describing: entry), String(reflecting: providers.keys), String(describing: providers.issues), providers.errorMessage ?? ""] {
			XCTAssertFalse(text.contains(secret), "the key text appeared in the model's values")
			XCTAssertFalse(text.contains(String(secret.suffix(6))), "not even a part of the key shows")
		}

		// Look through every file the run wrote.
		XCTAssertEqual(harness.session.appState.providerKeys().count, 1)
		for entry in entries {
			XCTAssertFalse(try Self.allText(in: URL(filePath: entry.rootPath)).contains(secret), "\(entry.name) folder holds the key")
			let settings = try TestFixtures.settingsJSON(in: URL(filePath: entry.rootPath))
			XCTAssertFalse(String(describing: settings).contains(secret))
		}
		XCTAssertFalse(try Self.allBytes(in: harness.container).contains(Data(secret.utf8)), "the app container holds the key outside the Keychain double")
		XCTAssertFalse(try Self.allBytes(in: harness.container).contains(Data(secret.suffix(12).utf8)))

		// A relaunch lists the key as stored without reading it back.
		let reopened = AiProvidersStepModel(session: try harness.relaunched().session)
		XCTAssertEqual(reopened.keys.map(\.name), ["Work key"])
		XCTAssertTrue(reopened.keys[0].isKeyStored)
	}

	func testRemovingAKeyDeletesItsSecretAndItsRecord() throws {
		let harness = try OnboardingHarness(self)
		let providers = AiProvidersStepModel(session: harness.session)
		var draft = ProviderKeyDraft(provider: .openRouter)
		draft.name = "Personal account"
		draft.key = "sk-or-v1-FAKEFAKEFAKEFAKEFAKEFAKE"
		guard case let .added(id) = providers.add(draft) else { return XCTFail("refused") }
		providers.remove(id: id)
		XCTAssertTrue(providers.keys.isEmpty)
		XCTAssertNil(try harness.session.environment.secrets.get(id: SecretId(kind: .providerApiKey, name: id)))
		XCTAssertTrue(harness.session.appState.providerKeys().isEmpty)
	}

	func testADraftIsCheckedBeforeAnythingIsStored() throws {
		let harness = try OnboardingHarness(self)
		let providers = AiProvidersStepModel(session: harness.session)
		var draft = ProviderKeyDraft(provider: .openAI)
		XCTAssertEqual(providers.add(draft), .refused(["name": "Name the key", "key": "Paste the API key"]))
		draft.name = "Home"
		draft.key = "two words"
		XCTAssertEqual(providers.add(draft), .refused(["key": "A key has no spaces or line breaks"]))
		draft.key = "sk-synthetic"
		draft.scope = .perWorkspace
		XCTAssertEqual(providers.add(draft), .refused(["scope": "Pick at least one workspace"]))
		var other = ProviderKeyDraft(provider: .other)
		other.name = "Gateway"
		other.key = "gw-synthetic"
		other.endpoint = "not a url"
		XCTAssertEqual(providers.add(other), .refused(["endpoint": "Use an address like http://localhost:11434"]))
		XCTAssertTrue(providers.keys.isEmpty)
		XCTAssertEqual(try harness.session.environment.secrets.list(kind: .providerApiKey), [], "a refused draft stored no secret")
	}

	func testALocalProviderTakesAnEndpointAndNoKey() throws {
		let harness = try OnboardingHarness(self)
		let providers = AiProvidersStepModel(session: harness.session)
		var draft = ProviderKeyDraft(provider: .local)
		draft.name = "Homelab"
		draft.endpoint = "http://nas.local:11434"
		guard case .added = providers.add(draft) else { return XCTFail("refused: \(providers.issues)") }
		XCTAssertEqual(providers.keys.first?.endpoint, "http://nas.local:11434")
		XCTAssertEqual(providers.keys.first?.isKeyStored, false)
		XCTAssertEqual(try harness.session.environment.secrets.list(kind: .providerApiKey), [])
		XCTAssertFalse(providers.isPaused(.local), "nothing leaves this Mac")
	}

	func testACloudProviderWithoutZeroRetentionIsPausedWhileZeroRetentionIsChosen() throws {
		let providers = AiProvidersStepModel(session: try OnboardingHarness(self).session)
		XCTAssertTrue(providers.isPaused(.claude))
		XCTAssertFalse(providers.isPaused(.openRouter))
		providers.setRetention(.safetyRetention)
		XCTAssertFalse(providers.isPaused(.claude))
	}

	func testTheMonthlyBudgetTakesAWholeNumberOrNothing() throws {
		let harness = try OnboardingHarness(self)
		let providers = AiProvidersStepModel(session: harness.session)
		XCTAssertTrue(providers.setMonthlyBudget("25"))
		XCTAssertEqual(providers.monthlyBudget, 25)
		XCTAssertFalse(providers.setMonthlyBudget("-3"))
		XCTAssertFalse(providers.setMonthlyBudget("ten"))
		XCTAssertEqual(providers.issues["budget"], "Use a whole number, or leave it empty for no limit")
		XCTAssertEqual(providers.monthlyBudget, 25)
		XCTAssertTrue(providers.setMonthlyBudget(""))
		XCTAssertNil(providers.monthlyBudget)
	}

	// MARK: Files

	private static func allBytes(in folder: URL) throws -> Data {
		var all = Data()
		let enumerator = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: [.isRegularFileKey])
		while let url = enumerator?.nextObject() as? URL {
			guard (try? url.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true else { continue }
			all.append(try Data(contentsOf: url))
		}
		return all
	}

	private static func allText(in folder: URL) throws -> String {
		String(decoding: try allBytes(in: folder), as: UTF8.self)
	}
}
