import Foundation
import Observation

/// A part of an account a workspace wants Vaultday to bring in.
public enum AccountPartChoice: String, CaseIterable, Identifiable, Hashable, Sendable {
	case mail
	case calendar
	case contacts
	case reminders

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .mail: "Mail"
		case .calendar: "Calendar"
		case .contacts: "Contacts"
		case .reminders: "Reminders"
		}
	}

	var core: AccountPartView {
		switch self {
		case .mail: .mail
		case .calendar: .calendar
		case .contacts: .contacts
		case .reminders: .reminders
		}
	}

	init(_ view: AccountPartView) {
		switch view {
		case .mail: self = .mail
		case .calendar: self = .calendar
		case .contacts: self = .contacts
		case .reminders: self = .reminders
		}
	}
}

/// How a mail account is reached.
public enum MailProviderChoice: String, CaseIterable, Identifiable, Hashable, Sendable {
	case imap
	case microsoft365

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .imap: "IMAP"
		case .microsoft365: "Microsoft 365"
		}
	}
}

/// What a mail folder is for.
public enum MailFolderRole: Hashable, Sendable {
	case inbox
	case sent
	case other
}

/// A folder the connection test found.
public struct MailFolderChoice: Identifiable, Hashable, Sendable {
	public let id: String
	public let name: String
	public let role: MailFolderRole
}

/// One connected mail account, as the Accounts panel lists it.
public struct MailAccountEntry: Identifiable, Hashable, Sendable {
	public let id: String
	public let address: String
	public let provider: MailProviderChoice
	public let workspace: String
	public let folders: [String]
	/// Whether this Mac holds the account's password or sign-in.
	public let hasCredential: Bool

	/// The line under the address: the protocol and the folders read.
	public var detail: String {
		let names = folders.map(Self.folderName)
		let base = "\(provider.title) · \(names.joined(separator: ", "))"
		return hasCredential ? base : "\(base) · sign in on this Mac"
	}

	private static func folderName(_ id: String) -> String {
		switch id.lowercased() {
		case "inbox": "Inbox"
		case "sentitems": "Sent Items"
		default: id
		}
	}
}

/// What the user typed for a new account. The password lives here only
/// until the connection test stores it in the Keychain.
public struct MailAccountDraft: Equatable, Sendable {
	public var workspace: String
	public var provider: MailProviderChoice = .imap
	public var address = ""
	public var host = ""
	public var port = 993
	public var password = ""
	/// The IMAP server wants Microsoft sign-in (XOAUTH2) instead of a password.
	public var usesMicrosoftSignIn = false

	public init(workspace: String) {
		self.workspace = workspace
	}

	var auth: MailAuthView {
		switch provider {
		case .microsoft365: .deviceCode
		case .imap: usesMicrosoftSignIn ? .xoAuth2 : .appPassword
		}
	}

	/// Whether the account signs in through Microsoft's device code rather than a password.
	public var needsDeviceCode: Bool { auth != .appPassword }
}

/// How the connection test ended, in words for the user.
public enum ConnectionTestResult: Equatable, Sendable {
	case connected(folders: [MailFolderChoice])
	case authFailed(String)
	case unreachable(String)
	case missingCredential(String)
	case notConfigured(String)
	case serverError(String)

	init(_ view: ConnectionTestView) {
		switch view.outcome {
		case .connected:
			self = .connected(folders: view.folders.map { folder in
				MailFolderChoice(id: folder.id, name: folder.name, role: MailFolderRole(folder.role))
			})
		case .authFailed: self = .authFailed(view.message)
		case .unreachable: self = .unreachable(view.message)
		case .missingCredential: self = .missingCredential(view.message)
		case .notConfigured: self = .notConfigured(view.message)
		case .serverError: self = .serverError(view.message)
		}
	}

	public var isConnected: Bool {
		if case .connected = self { return true }
		return false
	}

	/// The line shown under the Test button.
	public var summary: String {
		switch self {
		case let .connected(folders): "Connected · \(folders.count) folders found"
		case .authFailed: "The server refused the sign-in"
		case .unreachable: "The server could not be reached"
		case .missingCredential: "This Mac holds no password for the account"
		case .notConfigured: "Microsoft sign-in is not set up in this build"
		case .serverError: "The server answered something unexpected"
		}
	}

	/// The server's or the core's own words, when there are any.
	public var detail: String? {
		switch self {
		case .connected: nil
		case let .authFailed(message), let .unreachable(message), let .missingCredential(message),
		     let .notConfigured(message), let .serverError(message):
			message.isEmpty ? nil : message
		}
	}
}

/// Where the Microsoft device-code sign-in stands.
public enum MicrosoftSignInState: Equatable, Sendable {
	case idle
	case waiting(userCode: String, verificationURL: String)
	case signedIn
	case failed(String)
}

/// What the Accounts panel needs from the core's mail calls, which block on
/// the network; the real one runs them off the main actor, tests swap it.
protocol MailConnecting: Sendable {
	func test(_ account: MailAccountView) async -> ConnectionTestView
	func startSignIn(auth: MailAuthView) async throws -> DeviceCodePromptView
	func pollSignIn(connection: String, auth: MailAuthView, deviceCode: String) async throws -> DeviceCodePollView
}

/// The core's mail calls, each on a detached task.
struct CoreMailConnecting: MailConnecting {
	let appState: AppStateHandle

	func test(_ account: MailAccountView) async -> ConnectionTestView {
		let appState = appState
		return await Task.detached { appState.testMailConnection(account: account) }.value
	}

	func startSignIn(auth: MailAuthView) async throws -> DeviceCodePromptView {
		let appState = appState
		return try await Task.detached { try appState.startMicrosoftSignIn(auth: auth) }.value
	}

	func pollSignIn(connection: String, auth: MailAuthView, deviceCode: String) async throws -> DeviceCodePollView {
		let appState = appState
		return try await Task.detached { try appState.pollMicrosoftSignIn(connection: connection, auth: auth, deviceCode: deviceCode) }.value
	}
}

/// The Accounts step, and the same panel in Settings: the mail accounts of
/// each workspace (added, tested, their folders picked), and what each
/// workspace wants from its accounts. Passwords and sign-in tokens go to the
/// Keychain through the core, never to the app document or the vault.
@MainActor
@Observable
public final class AccountsStepModel {
	/// Whether an account can be added; mail accounts connect from this release.
	public let canAddAccounts = true
	/// The fetch intervals Settings offers, in minutes.
	public static let fetchIntervalChoices = [1, 5, 15, 30, 60]

	public private(set) var parts: [String: Set<AccountPartChoice>] = [:]
	public private(set) var accounts: [MailAccountEntry] = []
	public private(set) var errorMessage: String?
	/// The account being added, while the sheet is open.
	public var draft: MailAccountDraft?
	public private(set) var testResult: ConnectionTestResult?
	public private(set) var isTesting = false
	public private(set) var signIn: MicrosoftSignInState = .idle
	/// The folders picked for the draft, by id.
	public var selectedFolders: Set<String> = []
	public private(set) var fetchIntervalMinutes = 5
	/// Called after an account is added or removed, so the fetch schedule follows.
	public var onAccountsChange: (() -> Void)?

	private let session: CoreSession
	private let connecting: any MailConnecting
	private let notificationPermission: any NotificationPermissionAsking
	/// Whether this run asked for notifications already.
	public private(set) var hasAskedForNotifications = false
	/// The connection id of the draft; its secrets are stored under it.
	private var draftConnection: String?
	private var signInTask: Task<Void, Never>?

	public convenience init(session: CoreSession) {
		self.init(session: session, connecting: CoreMailConnecting(appState: session.appState))
	}

	init(session: CoreSession, connecting: any MailConnecting, notificationPermission: any NotificationPermissionAsking = SystemNotificationPermission()) {
		self.session = session
		self.connecting = connecting
		self.notificationPermission = notificationPermission
		reloadAccounts()
	}

	/// Whether any mail account exists; the Inbox column's empty state says so.
	public var hasMailAccount: Bool { !accounts.isEmpty }

	/// Whether Microsoft 365 sign-in can run in this build.
	public var isMicrosoftSignInAvailable: Bool { microsoftSignInIsConfigured() }

	/// Reads what each of the given workspaces wants, and every account.
	public func refresh(workspaces: [WorkspaceEntry]) {
		parts = Dictionary(uniqueKeysWithValues: workspaces.map { workspace in
			(workspace.id, Set(session.appState.accountParts(workspace: workspace.id).map(AccountPartChoice.init)))
		})
		reloadAccounts()
	}

	public func isOn(_ part: AccountPartChoice, workspace: String) -> Bool {
		parts[workspace, default: []].contains(part)
	}

	public func set(_ part: AccountPartChoice, isOn: Bool, workspace: String) {
		var chosen = parts[workspace, default: []]
		if isOn { chosen.insert(part) } else { chosen.remove(part) }
		do {
			let ordered = AccountPartChoice.allCases.filter { chosen.contains($0) }.map(\.core)
			try session.edit { try $0.setAccountParts(workspace: workspace, parts: ordered) }
			parts[workspace] = chosen
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// The accounts whose signals go to `workspace`.
	public func accounts(in workspace: String) -> [MailAccountEntry] {
		accounts.filter { $0.workspace == workspace }
	}

	// MARK: Adding an account

	/// Opens the sheet for a new account of `workspace`.
	public func beginAdding(workspace: String) {
		cancelAdding()
		draft = MailAccountDraft(workspace: workspace)
		draftConnection = "con-mail-\(UUID().uuidString.lowercased().prefix(13))"
	}

	/// Closes the sheet; a password or sign-in stored for the unsaved draft is forgotten.
	public func cancelAdding() {
		signInTask?.cancel()
		signInTask = nil
		if let draftConnection {
			try? session.appState.forgetMailCredentials(connection: draftConnection)
		}
		resetDraft()
	}

	/// The draft changed in a way the last test no longer covers.
	public func draftEdited() {
		testResult = nil
		selectedFolders = []
	}

	/// Stores the draft's password in the Keychain and signs in to list the folders.
	public func testConnection() async {
		guard let draft, let draftConnection else { return }
		if !draft.needsDeviceCode {
			do {
				try session.appState.storeMailPassword(connection: draftConnection, password: draft.password)
			} catch {
				testResult = .missingCredential(coreErrorMessage(error))
				return
			}
		}
		isTesting = true
		let view = await connecting.test(accountView(draft, connection: draftConnection, folders: []))
		isTesting = false
		guard self.draftConnection == draftConnection else { return }
		let result = ConnectionTestResult(view)
		testResult = result
		if case let .connected(folders) = result {
			selectedFolders = Set(folders.filter { $0.role != .other }.map(\.id))
		}
	}

	/// The folders the draft may read: Inbox and Sent Items.
	public var offeredFolders: [MailFolderChoice] {
		guard case let .connected(folders) = testResult else { return [] }
		return folders.filter { $0.role != .other }
	}

	/// Whether the draft can be saved: connected, and at least one folder picked.
	public var canSaveDraft: Bool {
		(testResult?.isConnected ?? false) && !selectedFolders.isEmpty
	}

	/// Starts the Microsoft sign-in and polls until it ends; a successful
	/// sign-in stores the refresh token in the Keychain and runs the test.
	public func startMicrosoftSignIn() {
		guard let draft, let draftConnection else { return }
		signInTask?.cancel()
		signIn = .idle
		let auth = draft.auth
		signInTask = Task { [weak self] in
			await self?.runSignIn(connection: draftConnection, auth: auth)
		}
	}

	/// Saves the draft and its folders to the app document.
	public func saveDraft() {
		guard let draft, let draftConnection, canSaveDraft else { return }
		let folders = offeredFolders.map(\.id).filter { selectedFolders.contains($0) }
		let account = accountView(draft, connection: draftConnection, folders: folders)
		do {
			try session.edit { try $0.putMailAccount(account: account) }
			errorMessage = nil
			resetDraft()
			reloadAccounts()
			askForNotificationsOnce()
			onAccountsChange?()
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// The quiet new-mail notification needs permission; it is asked here,
	/// right after an account is saved, so a background fetch never shows
	/// the system's request out of context. Once per run.
	private func askForNotificationsOnce() {
		guard !hasAskedForNotifications else { return }
		hasAskedForNotifications = true
		let permission = notificationPermission
		notificationRequest = Task { _ = await permission.requestAlerts() }
	}

	/// The permission request in flight, for tests to wait on.
	@ObservationIgnored private(set) var notificationRequest: Task<Void, Never>?

	/// Removes an account, its folders and this Mac's password or sign-in for it.
	public func removeAccount(_ id: String) {
		do {
			try session.edit { try $0.removeMailAccount(connection: id) }
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reloadAccounts()
		onAccountsChange?()
	}

	// MARK: Fetch interval

	/// Sets how often this and every other device fetches mail.
	public func setFetchInterval(minutes: Int) {
		do {
			try session.edit { try $0.setMailFetchIntervalSeconds(seconds: UInt64(max(minutes, 1) * 60)) }
			fetchIntervalMinutes = max(minutes, 1)
			errorMessage = nil
			onAccountsChange?()
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	// MARK: Private

	private func runSignIn(connection: String, auth: MailAuthView) async {
		do {
			let prompt = try await connecting.startSignIn(auth: auth)
			signIn = .waiting(userCode: prompt.userCode, verificationURL: prompt.verificationUri)
			var interval = max(prompt.intervalSeconds, 1)
			let deadline = Date().addingTimeInterval(TimeInterval(prompt.expiresInSeconds))
			while Date() < deadline {
				try await Task.sleep(for: .seconds(Int(interval)))
				switch try await connecting.pollSignIn(connection: connection, auth: auth, deviceCode: prompt.deviceCode) {
				case .pending:
					continue
				case .slowDown:
					interval += 5
				case .signedIn:
					signIn = .signedIn
					await testConnection()
					return
				case .declined:
					signIn = .failed("The sign-in was declined")
					return
				case .expired:
					signIn = .failed("The code expired; start the sign-in again")
					return
				}
			}
			signIn = .failed("The code expired; start the sign-in again")
		} catch is CancellationError {
			signIn = .idle
		} catch {
			signIn = .failed(coreErrorMessage(error))
		}
	}

	private func resetDraft() {
		draft = nil
		draftConnection = nil
		testResult = nil
		isTesting = false
		signIn = .idle
		selectedFolders = []
	}

	private func reloadAccounts() {
		accounts = session.appState.mailAccounts().map { view in
			MailAccountEntry(
				id: view.connection,
				address: view.address,
				provider: view.provider == .microsoftGraph ? .microsoft365 : .imap,
				workspace: view.workspace,
				folders: view.folders,
				hasCredential: (try? session.appState.hasMailCredential(account: view)) ?? false,
			)
		}
		fetchIntervalMinutes = Int(session.appState.mailFetchIntervalSeconds() / 60)
	}

	private func accountView(_ draft: MailAccountDraft, connection: String, folders: [String]) -> MailAccountView {
		let address = draft.address.trimmingCharacters(in: .whitespacesAndNewlines)
		let isImap = draft.provider == .imap
		return MailAccountView(
			connection: connection,
			label: address,
			workspace: draft.workspace,
			address: address,
			provider: isImap ? .imap : .microsoftGraph,
			host: isImap ? draft.host.trimmingCharacters(in: .whitespacesAndNewlines) : "",
			port: isImap ? UInt16(clamping: draft.port) : 0,
			auth: draft.auth,
			folders: folders,
		)
	}
}

extension MailFolderRole {
	init(_ view: MailFolderRoleView) {
		switch view {
		case .inbox: self = .inbox
		case .sent: self = .sent
		case .other: self = .other
		}
	}
}
