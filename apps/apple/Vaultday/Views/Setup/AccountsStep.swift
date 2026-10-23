import SwiftUI
import VaultdayCore

/// Step 5: accounts. Each workspace's mail accounts are added here (IMAP
/// with an app password, or Microsoft 365 through Microsoft's sign-in),
/// tested and given their folders; the same panel is in Settings.
struct AccountsStep: View {
	let workspaces: WorkspacesModel
	let accounts: AccountsStepModel
	/// Screen-share mode leaves the life workspaces out of the list.
	var showsSphere: (WorkspaceSphere) -> Bool = { _ in true }

	var body: some View {
		StepPane(title: "Your accounts", lede: "Add the accounts you use for mail, calendars and reminders to a workspace, then pick what Vaultday brings in from each") {
			AccountsPanel(workspaces: workspaces, accounts: accounts, showsSphere: showsSphere)
		}
	}
}

/// The workspaces with their mail accounts and parts, as the Accounts step
/// and Settings' Accounts tab both draw them.
struct AccountsPanel: View {
	let workspaces: WorkspacesModel
	let accounts: AccountsStepModel
	/// Screen-share mode leaves the life workspaces out of the list, in setup and in Settings.
	var showsSphere: (WorkspaceSphere) -> Bool = { _ in true }

	private let parts: [(AccountPartChoice, String, String)] = [
		(.mail, "envelope", "flagged and unread become inbox items"),
		(.calendar, "calendar", "events your day is planned around"),
		(.contacts, "person.2", "people on your tickets and mail"),
		(.reminders, "checklist", "items you add become tasks"),
	]

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
			SetupCallout(icon: "key", text: "Passwords and sign-ins stay in this Mac's Keychain; nothing secret goes to your notes or settings")
			if let error = accounts.errorMessage {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: error)
			}
			if workspaces.entries.isEmpty {
				SetupCallout(icon: "folder.badge.plus", text: "Add a workspace first; its accounts are listed here")
			}
			LazyVGrid(columns: [GridItem(.adaptive(minimum: 280), spacing: Tokens.Spacing.space16, alignment: .top)], alignment: .leading, spacing: Tokens.Spacing.space16) {
				ForEach(WorkspaceSphere.allCases.filter(showsSphere), id: \.self) { sphere in
					let inSphere = workspaces.entries(in: sphere)
					if !inSphere.isEmpty {
						SpherePanel(sphere: sphere) {
							EmptyView()
						} content: {
							ForEach(inSphere) { entry in workspaceCard(entry) }
						}
					}
				}
			}
		}
		.onAppear { accounts.refresh(workspaces: workspaces.entries) }
		.sheet(isPresented: Binding(get: { accounts.draft != nil }, set: { isShown in
			if !isShown { accounts.cancelAdding() }
		})) {
			AddMailAccountSheet(accounts: accounts)
				.mainWindowChildSheet()
		}
	}

	private func workspaceCard(_ entry: WorkspaceEntry) -> some View {
		let number = (workspaces.entries.firstIndex(of: entry) ?? 0) + 1
		let color = WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere)
		return VStack(spacing: 0) {
			HStack(spacing: Tokens.Spacing.space8) {
				WorkspaceBadge(number: number, color: color)
				Text(entry.name).font(Tokens.Typography.headline.font).foregroundStyle(Tokens.Colors.ink)
				SetupTag(text: "Active", tone: .on, icon: "checkmark")
				Spacer()
				Button { accounts.beginAdding(workspace: entry.id) } label: { SetupLabel(title: "Add", icon: "plus") }
					.buttonStyle(SetupButtonStyle(isSmall: true))
					.disabled(!accounts.canAddAccounts)
					.accessibilityLabel("Add an account to \(entry.name)")
			}
			.frame(minHeight: Tokens.Size.sizeRow)
			.padding(Tokens.Spacing.space12)
			ForEach(accounts.accounts(in: entry.id)) { account in
				accountRow(account, color: color.color)
					.padding(.horizontal, Tokens.Spacing.space12)
					.padding(.bottom, Tokens.Spacing.space8)
			}
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				Text("What \(entry.name) brings in")
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
				ForEach(parts, id: \.0) { part in
					Toggle(isOn: Binding(get: { accounts.isOn(part.0, workspace: entry.id) }, set: { accounts.set(part.0, isOn: $0, workspace: entry.id) })) {
						HStack(spacing: Tokens.Spacing.space8) {
							Image(systemName: part.1).frame(width: 16).foregroundStyle(Tokens.Colors.inkMuted)
							Text(part.0.title).font(Tokens.Typography.body.font.weight(.semibold)).foregroundStyle(Tokens.Colors.ink)
							Text(part.2).font(Tokens.Typography.callout.font).foregroundStyle(Tokens.Colors.inkMuted)
						}
					}
					.toggleStyle(.checkbox)
				}
			}
			.frame(maxWidth: .infinity, alignment: .leading)
			.padding(.horizontal, Tokens.Spacing.space12)
			.padding(.bottom, Tokens.Spacing.space12)
		}
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line, lineWidth: 1))
	}

	/// A nested account card: envelope, address, menu; the protocol and folders on line two.
	private func accountRow(_ account: MailAccountEntry, color: Color) -> some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
			HStack(spacing: Tokens.Spacing.space8) {
				Image(systemName: "envelope").foregroundStyle(color)
				Text(account.address)
					.font(Tokens.Typography.body.font.weight(.semibold))
					.foregroundStyle(Tokens.Colors.ink)
					.lineLimit(1)
				Spacer()
				Menu {
					Button("Remove account", role: .destructive) { accounts.removeAccount(account.id) }
				} label: {
					Image(systemName: "ellipsis")
				}
				.menuStyle(.borderlessButton)
				.fixedSize()
				.accessibilityLabel("Account options for \(account.address)")
			}
			Text(account.detail)
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(account.hasCredential ? Tokens.Colors.inkMuted : Tokens.Colors.caution)
		}
		.padding(Tokens.Spacing.space8)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(Tokens.Colors.surface, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineFaint, lineWidth: 1))
	}
}

/// Adds one mail account: the server and app password (or Microsoft's
/// sign-in), a connection test, and the folders to read.
struct AddMailAccountSheet: View {
	let accounts: AccountsStepModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
			HStack(alignment: .top, spacing: Tokens.Spacing.space12) {
				Image(systemName: "envelope").foregroundStyle(Tokens.Colors.primary).imageScale(.large)
				VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
					Text("Add a mail account").font(Tokens.Typography.headline.font)
					Text("Vaultday reads new mail from the folders you pick and never marks it read")
						.font(Tokens.Typography.callout.font).foregroundStyle(Tokens.Colors.inkSecondary)
				}
			}
			if accounts.draft != nil {
				SegmentedChoice(label: "Account type", options: MailProviderChoice.allCases.map { ($0, $0.title) }, selection: binding(\.provider))
				field("Address") {
					VimTextField("you@example.com", text: binding(\.address)).textFieldStyle(.roundedBorder)
				}
				if accounts.draft?.provider == .imap {
					imapFields
				}
				if accounts.draft?.needsDeviceCode == true {
					signInSection
				}
				testSection
				folderSection
			}
			HStack(spacing: Tokens.Spacing.space8) {
				Spacer()
				Button { accounts.cancelAdding() } label: { SetupLabel(title: "Cancel") }.buttonStyle(SetupButtonStyle(kind: .plain))
				Button { accounts.saveDraft() } label: { SetupLabel(title: "Add account") }
					.buttonStyle(SetupButtonStyle(kind: .primary))
					.keyboardShortcut(.defaultAction)
					.disabled(!accounts.canSaveDraft)
			}
		}
		.padding(Tokens.Spacing.space20)
		.frame(width: 560)
		.background(Tokens.Colors.surfaceRaised)
	}

	private var imapFields: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			HStack(alignment: .top, spacing: Tokens.Spacing.space12) {
				field("Server") {
					VimTextField("imap.example.com", text: binding(\.host)).textFieldStyle(.roundedBorder).font(Tokens.Typography.mono.font)
				}
				field("Port") {
					VimTextField("993", text: portText).textFieldStyle(.roundedBorder).frame(width: 80)
				}
			}
			Toggle("The server signs in with Microsoft (XOAUTH2)", isOn: binding(\.usesMicrosoftSignIn))
				.toggleStyle(.checkbox)
				.font(Tokens.Typography.callout.font)
			if accounts.draft?.usesMicrosoftSignIn == false {
				field("App password") {
					SecureField("From your provider's security settings", text: binding(\.password)).textFieldStyle(.roundedBorder)
				}
				Text("Stored in this Mac's Keychain when you test the connection")
					.font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
	}

	@ViewBuilder private var signInSection: some View {
		if !accounts.isMicrosoftSignInAvailable {
			SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: "Microsoft sign-in needs Vaultday's client id, which this build does not have yet")
		} else {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				Button { accounts.startMicrosoftSignIn() } label: { SetupLabel(title: "Sign in with Microsoft", icon: "person.badge.key") }
					.buttonStyle(SetupButtonStyle())
				switch accounts.signIn {
				case .idle:
					EmptyView()
				case let .waiting(userCode, verificationURL):
					HStack(spacing: Tokens.Spacing.space8) {
						Text("Enter").foregroundStyle(Tokens.Colors.inkSecondary)
						Text(userCode).font(Tokens.Typography.mono.font.weight(.semibold)).textSelection(.enabled)
						Text("at").foregroundStyle(Tokens.Colors.inkSecondary)
						if let url = URL(string: verificationURL) {
							Link(verificationURL, destination: url)
						}
					}
					.font(Tokens.Typography.callout.font)
				case .signedIn:
					SetupTag(text: "Signed in", tone: .on, icon: "checkmark")
				case let .failed(message):
					Text(message).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.caution)
				}
			}
		}
	}

	private var testSection: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HStack(spacing: Tokens.Spacing.space8) {
				Button {
					Task { await accounts.testConnection() }
				} label: { SetupLabel(title: "Test connection", icon: "bolt.horizontal") }
					.buttonStyle(SetupButtonStyle())
					.disabled(accounts.isTesting)
				if accounts.isTesting {
					ProgressView().controlSize(.small)
				}
				if let result = accounts.testResult {
					Text(result.summary)
						.font(Tokens.Typography.callout.font.weight(.medium))
						.foregroundStyle(result.isConnected ? Tokens.Colors.positive : Tokens.Colors.caution)
				}
			}
			if let detail = accounts.testResult?.detail {
				Text(detail).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted).lineLimit(3)
			}
		}
	}

	@ViewBuilder private var folderSection: some View {
		let offered = accounts.offeredFolders
		if !offered.isEmpty {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
				Text("Folders to read").font(Tokens.Typography.callout.font.weight(.semibold))
				ForEach(offered) { folder in
					Toggle(folder.name, isOn: Binding(get: { accounts.selectedFolders.contains(folder.id) }, set: { isOn in
						if isOn { accounts.selectedFolders.insert(folder.id) } else { accounts.selectedFolders.remove(folder.id) }
					}))
					.toggleStyle(.checkbox)
				}
			}
		}
	}

	/// A binding into the draft; editing it voids the last test.
	/// The port as text for the Vim field: digits only, an empty field keeps the last port.
	private var portText: Binding<String> {
		let port = binding(\.port)
		return Binding(
			get: { String(port.wrappedValue) },
			set: { text in
				if let value = Int(text.filter(\.isNumber)) { port.wrappedValue = value }
			},
		)
	}

	private func binding<Value: Equatable>(_ keyPath: WritableKeyPath<MailAccountDraft, Value>) -> Binding<Value> {
		Binding(
			get: { accounts.draft?[keyPath: keyPath] ?? MailAccountDraft(workspace: "")[keyPath: keyPath] },
			set: { value in
				guard accounts.draft?[keyPath: keyPath] != value else { return }
				accounts.draft?[keyPath: keyPath] = value
				accounts.draftEdited()
			},
		)
	}

	private func field(_ title: String, @ViewBuilder content: () -> some View) -> some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text(title).font(Tokens.Typography.callout.font.weight(.semibold))
			content()
		}
	}
}
