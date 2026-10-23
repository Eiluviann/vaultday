import SwiftUI
import VaultdayCore

/// The setup flow, filling the main window in place of Home: a 38 px title
/// bar over the 240 px step rail and the content pane with the shown step
/// and the sticky footer.
struct SetupScreen: View {
	let app: AppModel
	@FocusState private var hasKeyFocus: Bool

	var body: some View {
		VStack(spacing: 0) {
			titleBar
			if let setup = app.setup {
				SetupFrame(
					onboarding: setup.onboarding,
					scheme: Binding(get: { app.settings?.keyboardScheme ?? .standard }, set: { app.setKeyboardScheme($0) }),
					chooseRestoreFolder: { source in Task { await restore(source, setup: setup) } },
					finish: { app.finishSetup() },
				) {
					stepContent(setup)
				}
			} else {
				ContentUnavailableView("Vaultday could not start", systemImage: "exclamationmark.triangle", description: Text(app.setupError ?? "The app document is not open"))
					.frame(maxWidth: .infinity, maxHeight: .infinity)
			}
		}
		.background(Tokens.Colors.surfaceWindow)
		.ignoresSafeArea(.container, edges: .top)
		// Vim motions walk the steps and their controls; under Standard the
		// model is absent and AppKit's own keys stay as they were.
		.environment(\.setupKeyboard, app.dispatcher.scheme == .vim ? app.setupKeyboard : nil)
		.focusable()
		.focusEffectDisabled()
		.focused($hasKeyFocus)
		.defaultFocus($hasKeyFocus, true)
		.onKeyPress(phases: .down) { press in
			app.handleSetupKey(KeyStroke(press), isTyping: isTypingInTextField) ? .handled : .ignored
		}
		.onAppear { hasKeyFocus = true }
	}

	private var titleBar: some View {
		Text("Set up Vaultday")
			.font(Tokens.Typography.heading.font)
			.foregroundStyle(Tokens.Colors.ink)
			.frame(maxWidth: .infinity)
			.frame(height: SetupLayout.titleBarHeight)
			// The flow used to have its own window to close; Close leaves it for Home unfinished.
			.overlay(alignment: .trailing) {
				Button("Close") { app.mainWindow.showHome() }
					.buttonStyle(.plain)
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.padding(.trailing, Tokens.Spacing.space16)
					.help("Leave setup for Home; Set up Vaultday… in the Vaultday menu comes back")
			}
			.background(Tokens.Colors.surfaceSunken)
			.overlay(alignment: .bottom) { Rectangle().fill(Tokens.Colors.line).frame(height: 1) }
	}

	@ViewBuilder
	private func stepContent(_ setup: SetupModels) -> some View {
		switch setup.onboarding.current {
		case .welcome: WelcomeStep()
		case .workspaces: WorkspacesStep(workspaces: setup.workspaces, showsSphere: { app.screenShare.showsSphere($0) })
		case .folders: FoldersStep(workspaces: setup.workspaces)
		case .syncAndBackup: SyncStep(sync: setup.sync).onAppear { setup.sync.refreshGitWorkspaces() }
		case .accounts: AccountsStep(workspaces: setup.workspaces, accounts: setup.accounts, showsSphere: { app.screenShare.showsSphere($0) })
		case .featureSets: FeatureSetsStep(workspaces: setup.workspaces, features: setup.features)
		case .security: SecurityStep(workspaces: setup.workspaces, security: setup.security)
		case .rituals: RitualsStep(rhythm: setup.rhythm)
		case .aiLevel: AiLevelStep(workspaces: setup.workspaces, level: setup.aiLevel)
		case .aiProviders: AiProvidersStep(workspaces: setup.workspaces, providers: setup.aiProviders)
		case .aiAssistance: AiAssistanceStep(workspaces: setup.workspaces, assist: setup.aiAssistance)
		case .ready: ReadyStep(workspaces: setup.workspaces, ready: setup.ready)
		}
	}

	private func restore(_ source: RestoreSource, setup: SetupModels) async {
		let directory: URL? = source == .iCloud ? FolderPicker.iCloudDriveFolder : nil
		guard let url = await FolderPicker.pick(message: "Choose the folder that holds your workspaces", prompt: "Restore", canCreate: false, directory: directory) else { return }
		setup.onboarding.restore(from: url)
	}
}
