import SwiftUI
import VaultdayCore

/// The setup window: a 38 px title bar over the 240 px step rail and the
/// content pane with the shown step and the sticky footer.
struct SetupWindow: View {
	let app: AppModel

	var body: some View {
		VStack(spacing: 0) {
			titleBar
			if let setup = app.setup {
				SetupFrame(
					onboarding: setup.onboarding,
					scheme: Binding(get: { app.settings?.keyboardScheme ?? .standard }, set: { app.setKeyboardScheme($0) }),
					chooseRestoreFolder: { source in Task { await restore(source, setup: setup) } },
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
	}

	private var titleBar: some View {
		Text("Set up Vaultday")
			.font(Tokens.Typography.heading.font)
			.foregroundStyle(Tokens.Colors.ink)
			.frame(maxWidth: .infinity)
			.frame(height: SetupLayout.titleBarHeight)
			.background(Tokens.Colors.surfaceSunken)
			.overlay(alignment: .bottom) { Rectangle().fill(Tokens.Colors.line).frame(height: 1) }
	}

	@ViewBuilder
	private func stepContent(_ setup: SetupModels) -> some View {
		switch setup.onboarding.current {
		case .welcome: WelcomeStep()
		case .workspaces: WorkspacesStep(workspaces: setup.workspaces)
		case .folders: FoldersStep(workspaces: setup.workspaces)
		case .syncAndBackup: SyncStep(sync: setup.sync).onAppear { setup.sync.refreshGitWorkspaces() }
		case .accounts: AccountsStep(workspaces: setup.workspaces, accounts: setup.accounts)
		case .featureSets: FeatureSetsStep(workspaces: setup.workspaces, features: setup.features)
		case let step: LaterStep(step: step)
		}
	}

	private func restore(_ source: RestoreSource, setup: SetupModels) async {
		let directory: URL? = source == .iCloud ? FolderPicker.iCloudDriveFolder : nil
		guard let url = await FolderPicker.pick(message: "Choose the folder that holds your workspaces", prompt: "Restore", canCreate: false, directory: directory) else { return }
		setup.onboarding.restore(from: url)
	}
}
