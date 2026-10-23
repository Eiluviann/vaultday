import Foundation
import XCTest
@testable import VaultdayCore

/// The core's mail calls answered from a script: nothing reaches a mail server.
private final class ScriptedMail: MailConnecting, @unchecked Sendable {
	private let lock = NSLock()
	private var answer: ConnectionTestView
	private var tested: [MailAccountView] = []

	init(answer: ConnectionTestView) {
		self.answer = answer
	}

	var testedAccounts: [MailAccountView] {
		lock.withLock { tested }
	}

	func test(_ account: MailAccountView) async -> ConnectionTestView {
		lock.withLock {
			tested.append(account)
			return answer
		}
	}

	func startSignIn(auth _: MailAuthView) async throws -> DeviceCodePromptView {
		DeviceCodePromptView(deviceCode: "synthetic-device-code", userCode: "SYNTH-123", verificationUri: "https://microsoft.com/devicelogin", expiresInSeconds: 900, intervalSeconds: 1)
	}

	func pollSignIn(connection _: String, auth _: MailAuthView, deviceCode _: String) async throws -> DeviceCodePollView {
		.signedIn
	}
}

@MainActor
final class AccountsStepModelTests: XCTestCase {
	private static let connected = ConnectionTestView(
		outcome: .connected,
		message: "",
		folders: [
			MailFolderView(id: "INBOX", name: "Inbox", role: .inbox),
			MailFolderView(id: "Sent", name: "Sent", role: .sent),
			MailFolderView(id: "Archive", name: "Archive", role: .other),
		],
	)

	private func workspace(_ harness: OnboardingHarness) throws -> WorkspaceEntry {
		let workspaces = WorkspacesModel(session: harness.session)
		return try XCTUnwrap(workspaces.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Freelancing"), sphere: .work, name: "Freelancing"))
	}

	/// Every file under the App Group container, read whole.
	private func containerBytes(_ harness: OnboardingHarness) throws -> [Data] {
		let enumerator = try XCTUnwrap(FileManager.default.enumerator(at: harness.container, includingPropertiesForKeys: [.isRegularFileKey]))
		return enumerator.compactMap { $0 as? URL }.compactMap { try? Data(contentsOf: $0) }
	}

	func testThePlannedTagIsGoneAndAnAccountCanBeAdded() throws {
		let harness = try OnboardingHarness(self)
		let accounts = AccountsStepModel(session: harness.session, connecting: ScriptedMail(answer: Self.connected))
		XCTAssertTrue(accounts.canAddAccounts)
		XCTAssertFalse(accounts.hasMailAccount)
		XCTAssertNil(accounts.draft)
	}

	func testAnAppPasswordGoesToTheKeychainOnly() async throws {
		let harness = try OnboardingHarness(self)
		let entry = try workspace(harness)
		let mail = ScriptedMail(answer: Self.connected)
		let accounts = AccountsStepModel(session: harness.session, connecting: mail)
		let password = "synthetic-app-password-\(UUID().uuidString)"

		accounts.beginAdding(workspace: entry.id)
		accounts.draft?.address = "someone@example.test"
		accounts.draft?.host = "imap.example.test"
		accounts.draft?.password = password
		XCTAssertFalse(accounts.canSaveDraft, "nothing is saved before a test connects")
		await accounts.testConnection()

		// The connection test signed in with the stored password, not one passed along.
		let tested = try XCTUnwrap(mail.testedAccounts.first)
		XCTAssertEqual(tested.host, "imap.example.test")
		XCTAssertEqual(tested.port, 993)
		XCTAssertEqual(tested.auth, .appPassword)
		let item = try XCTUnwrap(harness.session.environment.secrets.get(id: SecretId(kind: .mailPassword, name: tested.connection)))
		XCTAssertEqual(String(decoding: item.value, as: UTF8.self), password)
		XCTAssertFalse(item.isSynchronisable, "the password stays on this Mac")

		// Inbox and Sent are offered and picked; other folders are not offered.
		XCTAssertEqual(accounts.offeredFolders.map(\.id), ["INBOX", "Sent"])
		XCTAssertEqual(accounts.selectedFolders, ["INBOX", "Sent"])
		accounts.selectedFolders.remove("Sent")
		XCTAssertTrue(accounts.canSaveDraft)
		accounts.saveDraft()
		XCTAssertNil(accounts.draft)

		let saved = harness.session.appState.mailAccounts()
		XCTAssertEqual(saved.count, 1)
		XCTAssertEqual(saved.first?.folders, ["INBOX"])
		XCTAssertEqual(saved.first?.workspace, entry.id)
		XCTAssertTrue(accounts.hasMailAccount)
		XCTAssertEqual(accounts.accounts(in: entry.id).first?.detail, "IMAP · Inbox")

		// The app document (the signed snapshot in the container) never holds the password.
		let needle = Data(password.utf8)
		for bytes in try containerBytes(harness) {
			XCTAssertNil(bytes.range(of: needle), "the password reached a file in the container")
		}
		let settings = try TestFixtures.settingsJSON(in: URL(fileURLWithPath: entry.rootPath))
		XCTAssertFalse(String(describing: settings).contains(password), "the password reached settings.json")

		// A relaunch reads the account back, with its password still in the Keychain.
		let relaunched = AccountsStepModel(session: try harness.relaunched().session, connecting: mail)
		XCTAssertEqual(relaunched.accounts.map(\.address), ["someone@example.test"])
		XCTAssertTrue(relaunched.accounts.first?.hasCredential ?? false)
	}

	func testNotificationsAreAskedForOnceWhenAnAccountIsSaved() async throws {
		let harness = try OnboardingHarness(self)
		let entry = try workspace(harness)
		let permission = CountingPermission()
		let accounts = AccountsStepModel(session: harness.session, connecting: ScriptedMail(answer: Self.connected), notificationPermission: permission)
		XCTAssertFalse(accounts.hasAskedForNotifications, "nothing is asked before an account exists")
		for address in ["one@example.test", "two@example.test"] {
			accounts.beginAdding(workspace: entry.id)
			accounts.draft?.address = address
			accounts.draft?.host = "imap.example.test"
			accounts.draft?.password = "synthetic-app-password"
			await accounts.testConnection()
			accounts.saveDraft()
		}
		await accounts.notificationRequest?.value
		XCTAssertEqual(accounts.accounts.count, 2)
		let asked = await permission.count
		XCTAssertEqual(asked, 1, "asked once, from the Accounts step")
	}

	func testRemovingAnAccountDeletesItsPassword() async throws {
		let harness = try OnboardingHarness(self)
		let entry = try workspace(harness)
		let accounts = AccountsStepModel(session: harness.session, connecting: ScriptedMail(answer: Self.connected))
		accounts.beginAdding(workspace: entry.id)
		accounts.draft?.address = "someone@example.test"
		accounts.draft?.host = "imap.example.test"
		accounts.draft?.password = "synthetic-app-password"
		await accounts.testConnection()
		accounts.saveDraft()
		let id = try XCTUnwrap(accounts.accounts.first?.id)

		accounts.removeAccount(id)
		XCTAssertFalse(accounts.hasMailAccount)
		XCTAssertNil(try harness.session.environment.secrets.get(id: SecretId(kind: .mailPassword, name: id)))
		XCTAssertTrue(harness.session.appState.mailAccounts().isEmpty)
	}

	func testCancellingForgetsTheUnsavedPassword() async throws {
		let harness = try OnboardingHarness(self)
		let entry = try workspace(harness)
		let accounts = AccountsStepModel(session: harness.session, connecting: ScriptedMail(answer: Self.connected))
		accounts.beginAdding(workspace: entry.id)
		accounts.draft?.address = "someone@example.test"
		accounts.draft?.host = "imap.example.test"
		accounts.draft?.password = "synthetic-app-password"
		await accounts.testConnection()
		XCTAssertEqual(try harness.session.environment.secrets.list(kind: .mailPassword).count, 1)

		accounts.cancelAdding()
		XCTAssertEqual(try harness.session.environment.secrets.list(kind: .mailPassword), [])
		XCTAssertTrue(harness.session.appState.mailAccounts().isEmpty)
	}

	func testConnectionResultsMapToWhatTheUserIsTold() {
		let connected = ConnectionTestResult(Self.connected)
		XCTAssertTrue(connected.isConnected)
		XCTAssertEqual(connected.summary, "Connected · 3 folders found")
		XCTAssertNil(connected.detail)

		let cases: [(MailOutcomeView, ConnectionTestResult, String)] = [
			(.authFailed, .authFailed("NO [AUTHENTICATIONFAILED]"), "The server refused the sign-in"),
			(.unreachable, .unreachable("NO [AUTHENTICATIONFAILED]"), "The server could not be reached"),
			(.missingCredential, .missingCredential("NO [AUTHENTICATIONFAILED]"), "This Mac holds no password for the account"),
			(.notConfigured, .notConfigured("NO [AUTHENTICATIONFAILED]"), "Microsoft sign-in is not set up in this build"),
			(.serverError, .serverError("NO [AUTHENTICATIONFAILED]"), "The server answered something unexpected"),
		]
		for (outcome, expected, summary) in cases {
			let result = ConnectionTestResult(ConnectionTestView(outcome: outcome, message: "NO [AUTHENTICATIONFAILED]", folders: []))
			XCTAssertEqual(result, expected)
			XCTAssertFalse(result.isConnected)
			XCTAssertEqual(result.summary, summary)
			XCTAssertFalse(result.summary.hasSuffix("."), "single-sentence labels end without a period")
			XCTAssertEqual(result.detail, "NO [AUTHENTICATIONFAILED]")
		}
	}

	func testAFailedTestOffersNoFoldersAndCannotBeSaved() async throws {
		let harness = try OnboardingHarness(self)
		let entry = try workspace(harness)
		let refused = ConnectionTestView(outcome: .authFailed, message: "Invalid credentials", folders: [])
		let accounts = AccountsStepModel(session: harness.session, connecting: ScriptedMail(answer: refused))
		accounts.beginAdding(workspace: entry.id)
		accounts.draft?.address = "someone@example.test"
		accounts.draft?.host = "imap.example.test"
		accounts.draft?.password = "wrong"
		await accounts.testConnection()
		XCTAssertEqual(accounts.testResult, .authFailed("Invalid credentials"))
		XCTAssertTrue(accounts.offeredFolders.isEmpty)
		XCTAssertFalse(accounts.canSaveDraft)
		accounts.saveDraft()
		XCTAssertTrue(harness.session.appState.mailAccounts().isEmpty)
	}

	func testMicrosoft365SignsInWithTheDeviceCodeAndNeedsNoPassword() async throws {
		let harness = try OnboardingHarness(self)
		let entry = try workspace(harness)
		let graphFolders = ConnectionTestView(
			outcome: .connected,
			message: "",
			folders: [MailFolderView(id: "inbox", name: "Inbox", role: .inbox), MailFolderView(id: "sentitems", name: "Sent Items", role: .sent)],
		)
		let mail = ScriptedMail(answer: graphFolders)
		let accounts = AccountsStepModel(session: harness.session, connecting: mail)
		accounts.beginAdding(workspace: entry.id)
		accounts.draft?.provider = .microsoft365
		accounts.draft?.address = "someone@contoso.example.test"
		XCTAssertTrue(accounts.draft?.needsDeviceCode ?? false)

		await accounts.testConnection()
		XCTAssertEqual(try harness.session.environment.secrets.list(kind: .mailPassword), [], "Microsoft 365 stores no password")
		let tested = try XCTUnwrap(mail.testedAccounts.first)
		XCTAssertEqual(tested.provider, .microsoftGraph)
		XCTAssertEqual(tested.auth, .deviceCode)
		XCTAssertEqual(tested.host, "")
		accounts.saveDraft()
		XCTAssertEqual(accounts.accounts.first?.detail.hasPrefix("Microsoft 365 · Inbox, Sent Items"), true)
	}

	func testTheFetchIntervalStartsAtFiveMinutes() throws {
		let harness = try OnboardingHarness(self)
		let accounts = AccountsStepModel(session: harness.session, connecting: ScriptedMail(answer: Self.connected))
		XCTAssertEqual(accounts.fetchIntervalMinutes, 5)
		accounts.setFetchInterval(minutes: 15)
		XCTAssertEqual(accounts.fetchIntervalMinutes, 15)
		XCTAssertEqual(harness.session.appState.mailFetchIntervalSeconds(), 900)
	}
}

/// Counts the permission requests instead of showing the system's.
private actor CountingPermission: NotificationPermissionAsking {
	private(set) var count = 0

	func requestAlerts() async -> Bool {
		count += 1
		return true
	}
}
