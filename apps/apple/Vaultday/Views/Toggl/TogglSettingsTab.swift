import SwiftUI
import VaultdayCore

/// The Toggl and billing tab of Settings: per workspace, the switch, the
/// token (written to the Keychain, shown only as "Token stored") and where
/// entries go in Toggl, typed by hand while live access is not allowed.
struct TogglSettingsTab: View {
	/// Builds one panel per workspace; called when the tab appears, so a
	/// token being typed survives the window redrawing.
	let makePanels: () -> [TogglSettingsModel]
	let queue: TogglQueueModel?
	/// Screen-share mode: the panels follow it while Settings is open.
	let isScreenShareOn: Bool
	@State private var panels: [TogglSettingsModel] = []

	var body: some View {
		ScrollView {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
				SetupCallout(icon: "lock.shield", text: "The token stays in the Keychain on this Mac; Toggl gets durations, projects and tags, never rates")
				if let queue {
					SetupCard(icon: "tray.full", title: "Toggl queue", description: queue.statusText) {
						Button {
							Task { await queue.drainNow() }
						} label: {
							SetupLabel(title: "Send now", icon: "arrow.up.circle")
						}
						.buttonStyle(SetupButtonStyle(isSmall: true))
						.disabled(queue.isDraining || queue.queuedCount == 0)
						.accessibilityLabel("Send the Toggl queue now")
					}
				}
				if panels.isEmpty {
					Text("No workspace yet")
						.textStyle(Tokens.Typography.callout)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
				ForEach(panels, id: \.workspaceId) { panel in
					TogglWorkspacePanel(panel: panel)
				}
			}
			.padding(Tokens.Spacing.space24)
			.frame(maxWidth: .infinity, alignment: .leading)
		}
		.onAppear {
			panels = makePanels()
			queue?.reload()
		}
		.onChange(of: isScreenShareOn) {
			panels = TogglSettingsModel.refreshed(panels, with: makePanels())
		}
	}
}

private struct TogglWorkspacePanel: View {
	@Bindable var panel: TogglSettingsModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			SetupCard(icon: "clock.badge.checkmark", title: panel.workspaceName, description: panel.isEnabled ? "Stopped sessions go to Toggl as the billing model says" : "Off for this workspace") {
				SetupSwitch(label: "Toggl for \(panel.workspaceName)", isOn: Binding(get: { panel.isEnabled }, set: { panel.setEnabled($0) }))
			}
			if let error = panel.errorMessage {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: error)
			}
			if panel.isEnabled {
				token
				mapping
			}
		}
	}

	private var token: some View {
		SetupCard(icon: "key", title: "API token", description: panel.tokenStatusText) {
			if panel.hasToken {
				Button {
					panel.removeToken()
				} label: {
					SetupLabel(title: "Remove", icon: "trash")
				}
				.buttonStyle(SetupButtonStyle(isSmall: true))
				.accessibilityLabel("Remove the Toggl token")
			}
		} extra: {
			HStack(spacing: Tokens.Spacing.space8) {
				SecureField(panel.hasToken ? "Paste a new token to replace it" : "Paste the token from your Toggl profile", text: $panel.tokenDraft)
					.textFieldStyle(.roundedBorder)
					.accessibilityLabel("Toggl API token")
				Button("Store") { panel.storeToken() }
					.buttonStyle(SetupButtonStyle(isSmall: true))
					.disabled(panel.tokenDraft.isEmpty)
			}
		}
	}

	private var mapping: some View {
		SetupCard(icon: "arrow.triangle.branch", title: "Where entries go", description: panel.canFetchFromToggl ? "Pick from your Toggl account or type the ids" : TogglSettingsModel.typedByHandNote) {
			Button {
				panel.fetchFromToggl()
			} label: {
				SetupLabel(title: "Fetch from Toggl", icon: "arrow.down.circle")
			}
			.buttonStyle(SetupButtonStyle(isSmall: true))
			.disabled(!panel.canFetchFromToggl)
			.help(panel.canFetchFromToggl ? "Reads your Toggl workspaces and projects" : "Live access to Toggl is not allowed yet")
		} extra: {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
				idField("Toggl workspace id", text: $panel.togglWorkspaceId, choices: panel.togglWorkspaces)
				idField("Default project id", text: $panel.defaultProjectId, choices: panel.togglProjects)
				ForEach($panel.storyProjects) { $entry in
					HStack(spacing: Tokens.Spacing.space6) {
						TextField("Story", text: $entry.story)
							.textFieldStyle(.roundedBorder)
							.textStyle(Tokens.Typography.mono)
							.frame(width: 140)
						TextField("Project id", text: $entry.projectId)
							.textFieldStyle(.roundedBorder)
							.textStyle(Tokens.Typography.mono)
					}
				}
				HStack {
					Button("Add a story project") { panel.addStoryProject() }
						.buttonStyle(SetupButtonStyle(kind: .plain, isSmall: true))
					Spacer()
					Button("Save mapping") { panel.saveMapping() }
						.buttonStyle(SetupButtonStyle(isSmall: true))
				}
			}
		}
	}

	private func idField(_ label: String, text: Binding<String>, choices: [TogglChoice]) -> some View {
		HStack(spacing: Tokens.Spacing.space6) {
			Text(label)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.frame(width: 140, alignment: .leading)
			TextField("Id", text: text)
				.textFieldStyle(.roundedBorder)
				.textStyle(Tokens.Typography.mono)
				.accessibilityLabel(label)
			if !choices.isEmpty {
				Menu("Pick") {
					ForEach(choices) { choice in
						Button(choice.name) { text.wrappedValue = String(choice.id) }
					}
				}
				.fixedSize()
			}
		}
	}
}
