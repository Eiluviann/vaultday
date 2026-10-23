import SwiftUI
import VaultdayCore

/// The Settings window (⌘,): the app-level choices the setup steps made, in
/// the same controls, and each workspace's Configure sheet. Every control
/// writes through the core at once; there is no Save.
struct SettingsWindow: View {
	let app: AppModel

	var body: some View {
		Group {
			if let settings = app.settings, let rhythm = app.rhythm, let workspaces = app.workspaces {
				TabView {
					GeneralSettings(app: app, settings: settings)
						.tabItem { Label("General", systemImage: "gearshape") }
					DaySettings(rhythm: rhythm)
						.tabItem { Label("Day", systemImage: "sun.max") }
					WorkspaceSettings(workspaces: workspaces, billing: { app.billingModel(for: $0) }, onChange: { app.workspacesChanged() })
						.tabItem { Label("Workspaces", systemImage: "square.stack") }
					if let accounts = app.setup?.accounts {
						AccountSettings(workspaces: workspaces, accounts: accounts, scheduler: app.mailScheduler)
							.tabItem { Label("Accounts", systemImage: "envelope") }
					}
					TogglSettingsTab(makePanels: { app.togglPanels() }, queue: app.togglQueue)
						.tabItem { Label("Toggl and billing", systemImage: "clock.badge.checkmark") }
					DeviceSettings(settings: settings)
						.tabItem { Label("This Mac", systemImage: "desktopcomputer") }
				}
			} else {
				ContentUnavailableView("Settings are not available yet", systemImage: "gearshape", description: Text(app.setupError ?? "The app document is not open"))
			}
		}
		.frame(width: 760, height: 620)
		.background(Tokens.Colors.surface)
	}
}

// MARK: General

private struct GeneralSettings: View {
	let app: AppModel
	let settings: SettingsModel

	var body: some View {
		SettingsPane(error: settings.errorMessage) {
			SetupCard(icon: "keyboard", title: "Keyboard scheme", description: "Vim motions move through lists with the keys you know; Standard uses the arrows") {
				SegmentedChoice(
					label: "Keyboard scheme",
					options: [(KeyboardSchemeSetting.standard, "Standard"), (.vimMotions, "Vim motions")],
					selection: Binding(get: { settings.keyboardScheme }, set: { app.setKeyboardScheme($0) }),
				)
			}
			SetupCard(icon: "cursorarrow.motionlines", title: "Hover selects", description: "Pointing at a row selects it, on this Mac") {
				SetupSwitch(label: "Hover selects", isOn: Binding(get: { settings.hoverSelects }, set: { app.setHoverSelects($0) }))
			}
			SetupCard(icon: "terminal", title: "Command line", description: app.localAPI?.statusText ?? "The localhost API has not started", footnote: "vaultday timer and vaultday status read the token in the App Group container") {
				EmptyView()
			} extra: {
				EmptyView()
			}
			SetupCard(icon: "command", title: "Shortcut caps", description: "How the shortcut caps arrive with the selection; Reduce Motion always crossfades") {
				SegmentedChoice(
					label: "Shortcut caps",
					options: ShortcutCapsSetting.allCases.map { ($0, $0.title) },
					selection: Binding(get: { settings.shortcutCaps }, set: { settings.setShortcutCaps($0) }),
				)
			}
		}
	}
}

// MARK: Day

private struct DaySettings: View {
	let rhythm: RhythmModel

	var body: some View {
		SettingsPane(error: rhythm.errorMessage) {
			SetupCard(icon: "gauge.with.dots.needle.67percent", title: "Workload threshold", description: "The planned work a day may hold before planning warns") {
				Stepper(value: Binding(get: { rhythm.workloadThresholdMinutes }, set: { rhythm.setWorkloadThreshold(minutes: $0) }), in: RhythmModel.minimumWorkloadMinutes ... RhythmModel.maximumWorkloadMinutes, step: 30) {
					Text(Self.duration(rhythm.workloadThresholdMinutes))
						.font(Tokens.Typography.body.font.monospacedDigit())
						.frame(minWidth: 70, alignment: .trailing)
				}
				.accessibilityLabel("Workload threshold")
			} extra: {
				if let issue = rhythm.issues["workload"] {
					Text(issue).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.caution)
				}
			}
			SetupCard(icon: "timer", title: "Long-running timer", description: "A session running this long, or past the workload threshold, asks whether you are still working on it") {
				Stepper(value: Binding(get: { rhythm.longTimerLimitMinutes }, set: { rhythm.setLongTimerLimit(minutes: $0) }), in: RhythmModel.minimumLongTimerMinutes ... RhythmModel.maximumLongTimerMinutes, step: 15) {
					Text(Self.duration(rhythm.longTimerLimitMinutes))
						.font(Tokens.Typography.body.font.monospacedDigit())
						.frame(minWidth: 70, alignment: .trailing)
				}
				.accessibilityLabel("Long-running timer limit")
			} extra: {
				if let issue = rhythm.issues["longTimer"] {
					Text(issue).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.caution)
				}
			}
			SetupCard(icon: "plus.circle", title: "New tasks", description: "Where a task you add lands in a list") {
				SetupMenu(label: "New tasks", options: ListPositionSetting.allCases.map { ($0, $0.newTaskTitle) }, selection: Binding(get: { rhythm.newTaskPosition }, set: { rhythm.setNewTaskPosition($0) }), width: 130)
			}
			SetupCallout(text: "Rituals open from a later release; the times you set are remembered for then") { PlannedTag() }
			RhythmSections(rhythm: rhythm, includesLife: true)
		}
	}

	/// `8 h`, `7 h 30 min`, `45 min`.
	static func duration(_ minutes: Int) -> String {
		let hours = minutes / 60, rest = minutes % 60
		switch (hours, rest) {
		case (0, _): return "\(rest) min"
		case (_, 0): return "\(hours) h"
		default: return "\(hours) h \(rest) min"
		}
	}
}

// MARK: Workspaces

private struct WorkspaceSettings: View {
	let workspaces: WorkspacesModel
	/// The billing fields of a workspace's sheet, while Toggl and billing is on for it.
	let billing: (WorkspaceEntry) -> BillingSettingsModel?
	let onChange: () -> Void
	@State private var configuring: ConfigureTarget?

	var body: some View {
		SettingsPane(error: workspaces.message) {
			ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
				SpherePanel(sphere: sphere) { EmptyView() } content: {
					let inSphere = workspaces.entries(in: sphere)
					if inSphere.isEmpty {
						Text(sphere == .work ? "No work workspace yet" : "No life workspace yet")
							.font(Tokens.Typography.callout.font)
							.foregroundStyle(Tokens.Colors.inkMuted)
							.padding(.vertical, Tokens.Spacing.space8)
					}
					ForEach(inSphere) { entry in
						SetupCard(title: entry.name, description: entry.rootPath.abbreviatedPath) {
							Button {
								configuring = ConfigureTarget(model: WorkspaceConfigureModel(root: entry.rootPath), billing: billing(entry))
							} label: {
								SetupLabel(title: "Configure", icon: "gearshape")
							}
							.buttonStyle(SetupButtonStyle(isSmall: true))
							.accessibilityLabel("Configure \(entry.name)")
						}
					}
				}
			}
			Text("Add, remove and restore workspaces in the setup window, from the Vaultday menu")
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
		.onAppear { workspaces.reload() }
		.sheet(item: $configuring) { target in
			ConfigureSheet(model: target.model, billing: target.billing, onDone: {
				workspaces.reload()
				onChange()
			})
		}
	}
}

// MARK: Accounts

/// The Accounts step's panel, and how often mail is fetched.
private struct AccountSettings: View {
	let workspaces: WorkspacesModel
	let accounts: AccountsStepModel
	let scheduler: MailFetchScheduler?

	var body: some View {
		SettingsPane {
			SetupCard(icon: "arrow.clockwise", title: "Fetch mail", description: "How often each Mac checks its accounts for new mail") {
				SegmentedChoice(
					label: "Fetch interval",
					options: AccountsStepModel.fetchIntervalChoices.map { ($0, $0 == 60 ? "1 h" : "\($0) min") },
					selection: Binding(get: { accounts.fetchIntervalMinutes }, set: { accounts.setFetchInterval(minutes: $0) }),
				)
			}
			if let scheduler {
				SetupCard(icon: "tray.and.arrow.down", title: "Fetch now", description: scheduler.lastFetch.map(Self.lastFetchLine) ?? "Not fetched yet in this session") {
					Button {
						Task { await scheduler.fetchNow() }
					} label: {
						SetupLabel(title: "Fetch", icon: "arrow.clockwise")
					}
					.buttonStyle(SetupButtonStyle(isSmall: true))
					.disabled(scheduler.isFetching || !accounts.hasMailAccount)
					.accessibilityLabel("Fetch mail now")
				}
			}
			AccountsPanel(workspaces: workspaces, accounts: accounts)
		}
	}

	private static func lastFetchLine(_ summary: MailFetchSummary) -> String {
		let time = summary.finishedAt.formatted(date: .omitted, time: .shortened)
		guard summary.failures.isEmpty else {
			return "Last fetch at \(time) · \(summary.failures.joined(separator: "; "))"
		}
		return "Last fetch at \(time) · \(summary.added) new"
	}
}

// MARK: This Mac

private struct DeviceSettings: View {
	let settings: SettingsModel

	var body: some View {
		SettingsPane(error: settings.errorMessage) {
			SetupCard(icon: "number", title: "Device id", description: SettingsModel.deviceIdNote) {
				Button {
					settings.copyDeviceId()
				} label: {
					SetupLabel(title: "Copy", icon: "doc.on.doc")
				}
				.buttonStyle(SetupButtonStyle(isSmall: true))
				.accessibilityLabel("Copy the device id")
			} extra: {
				Text(settings.deviceId)
					.font(Tokens.Typography.mono.font)
					.textSelection(.enabled)
					.foregroundStyle(Tokens.Colors.ink)
					.padding(Tokens.Spacing.space8)
					.frame(maxWidth: .infinity, alignment: .leading)
					.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
					.accessibilityLabel("Device id \(settings.deviceId)")
			}
			if !settings.connections.isEmpty {
				SetupCard(icon: "link", title: "Connections", description: "\(settings.connections.count) in the app document") { EmptyView() }
			}
			if !settings.refusedSnapshots.isEmpty {
				ForEach(settings.refusedSnapshots, id: \.self) { refused in
					SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: refused)
				}
			}
		}
	}
}

// MARK: Pane

/// A scrolling tab body at the setup steps' measure.
private struct SettingsPane<Content: View>: View {
	var error: String?
	@ViewBuilder var content: Content

	var body: some View {
		ScrollView {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
				if let error {
					SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: error)
				}
				content
			}
			.padding(Tokens.Spacing.space24)
			.frame(maxWidth: .infinity, alignment: .leading)
		}
	}
}
