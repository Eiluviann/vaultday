import SwiftUI
import VaultdayCore

/// The Configure sheet (board 2a, 860 × 680): the fields of the workspace's
/// `.vaultday/settings.json`. The core checks every value; its message sits
/// under the field it is about. What the file cannot hold (security, AI,
/// accounts) is not on this sheet.
struct ConfigureSheet: View {
	enum Tab: String, CaseIterable, Identifiable {
		case general = "General"
		case folders = "Folders"
		case tasks = "Tasks and tickets"
		case hours = "Hours"

		var id: String { rawValue }

		var symbol: String {
			switch self {
			case .general: "gearshape"
			case .folders: "folder"
			case .tasks: "checklist"
			case .hours: "clock"
			}
		}
	}

	@Bindable var model: WorkspaceConfigureModel
	/// The billing block and rate; `nil` while the Toggl and billing feature set is off.
	var billing: BillingSettingsModel?
	/// The close-out keys: `log_deadline` and the leave source.
	var closeOut: CloseOutSettingsModel?
	/// Workspaces whose `Leave.md` this one may read, by name.
	var leaveSources: [String] = []
	let onDone: () -> Void
	@Environment(\.dismiss) private var dismiss
	@State private var tab: Tab = .general

	var body: some View {
		VStack(spacing: 0) {
			header
			Divider().overlay(Tokens.Colors.line)
			HStack(spacing: 0) {
				tabList
				Divider().overlay(Tokens.Colors.line)
				ScrollView {
					VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
						if let error = model.errorMessage {
							SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: error)
						}
						tabContent
					}
					.padding(Tokens.Spacing.space24)
					.frame(maxWidth: .infinity, alignment: .leading)
				}
			}
			Divider().overlay(Tokens.Colors.line)
			footer
		}
		.frame(width: SetupLayout.configureSheetSize.width, height: SetupLayout.configureSheetSize.height)
		.background(Tokens.Colors.surface)
		.onExitCommand { dismiss() }
	}

	// MARK: Frame

	private var header: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Text("Configure \(model.name)")
				.textStyle(Tokens.Typography.title)
				.foregroundStyle(Tokens.Colors.ink)
				.accessibilityAddTraits(.isHeader)
			SetupTag(text: model.sphere == .work ? "Work sphere" : "Life sphere", tone: model.sphere == .work ? .work : .life, icon: model.sphere == .work ? "briefcase" : "leaf")
			Spacer()
		}
		.padding(.horizontal, Tokens.Spacing.space24)
		.padding(.vertical, Tokens.Spacing.space16)
	}

	private var tabList: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
			ForEach(Tab.allCases) { entry in
				Button {
					tab = entry
				} label: {
					HStack(spacing: Tokens.Spacing.space8) {
						Image(systemName: entry.symbol).frame(width: 18)
						Text(entry.rawValue).font(Tokens.Typography.body.font.weight(tab == entry ? .semibold : .medium))
						Spacer(minLength: 0)
						if entry != tab, hasIssue(in: entry) {
							Image(systemName: "exclamationmark.circle.fill").foregroundStyle(Tokens.Colors.danger)
						}
					}
					.foregroundStyle(tab == entry ? Tokens.Colors.ink : Tokens.Colors.inkSecondary)
					.padding(.horizontal, Tokens.Spacing.space8)
					.padding(.vertical, 6)
					.background(tab == entry ? Tokens.Colors.surfaceRaised : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
					.contentShape(Rectangle())
				}
				.buttonStyle(.plain)
				.accessibilityAddTraits(tab == entry ? .isSelected : [])
			}
			Spacer()
		}
		.padding(Tokens.Spacing.space12)
		.frame(width: 200)
		.frame(maxHeight: .infinity)
		.background(Tokens.Colors.surfaceSidebar)
	}

	private var footer: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space8) {
				Image(systemName: "doc.text").foregroundStyle(Tokens.Colors.inkMuted)
				Text("Every value here is saved to ")
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					+ Text(model.settingsPath.abbreviatedPath)
					.font(Tokens.Typography.mono.font)
					.foregroundStyle(Tokens.Colors.ink)
				Spacer()
				Button("Open in editor") {
					NSWorkspace.shared.open(URL(filePath: model.settingsPath))
				}
				.buttonStyle(SetupButtonStyle(kind: .plain, isSmall: true))
			}
			HStack(spacing: Tokens.Spacing.space8) {
				Text("Security, integrations and AI for this workspace are kept in Vaultday, not in the file, so an edit to settings.json can never loosen them")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.fixedSize(horizontal: false, vertical: true)
				Spacer(minLength: Tokens.Spacing.space16)
				Button("Cancel") { dismiss() }
					.buttonStyle(SetupButtonStyle())
					.keyboardShortcut(.cancelAction)
				Button("Done") {
					if model.save(), billing?.save() ?? true, closeOut?.save() ?? true {
						onDone()
						dismiss()
					} else if let first = firstTabWithIssue {
						tab = first
					}
				}
				.buttonStyle(SetupButtonStyle(kind: .primary))
				.keyboardShortcut(.defaultAction)
				.disabled(!model.isLoaded)
			}
		}
		.padding(.horizontal, Tokens.Spacing.space24)
		.padding(.vertical, Tokens.Spacing.space12)
	}

	// MARK: Tabs

	@ViewBuilder
	private var tabContent: some View {
		switch tab {
		case .general: general
		case .folders: folders
		case .tasks: tasks
		case .hours: hours
		}
	}

	private var general: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
			field("Name", key: "name") {
				TextField("Name", text: $model.name).textFieldStyle(.roundedBorder)
			}
			field("Color", key: "accent", note: "The color of the workspace's badge and dot") {
				HStack(spacing: Tokens.Spacing.space8) {
					ForEach(WorkspaceColor.allCases) { color in
						let isChosen = model.accent == color.rawValue
						Button {
							model.accent = color.rawValue
						} label: {
							Circle().fill(color.color).frame(width: 20, height: 20)
								.padding(2)
								.overlay(Circle().strokeBorder(isChosen ? Tokens.Colors.ink : .clear, lineWidth: 2))
						}
						.buttonStyle(.plain)
						.accessibilityLabel(color.rawValue.capitalized)
						.accessibilityAddTraits(isChosen ? .isSelected : [])
					}
					if !model.accent.isEmpty, WorkspaceColor(rawValue: model.accent) == nil {
						Text(model.accent).textStyle(Tokens.Typography.mono).foregroundStyle(Tokens.Colors.inkMuted)
					}
				}
			}
			field("Browser", key: "browser", note: "Opens this workspace's links; empty uses the system browser") {
				TextField("System default", text: $model.browser).textFieldStyle(.roundedBorder)
			}
		}
	}

	private var folders: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			Text("The workspace's folder")
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkSecondary)
			HStack(spacing: Tokens.Spacing.space8) {
				Image(systemName: "folder").foregroundStyle(Tokens.Colors.inkMuted)
				Text(model.root.abbreviatedPath).textStyle(Tokens.Typography.mono).foregroundStyle(Tokens.Colors.ink)
			}
			.padding(Tokens.Spacing.space12)
			.frame(maxWidth: .infinity, alignment: .leading)
			.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
			Text("Further folders are added on the Folders step")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
	}

	private var tasks: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
			field("Ticket ids", key: "ticket_pattern", note: "A regular expression; it finds ids such as WEB-142 in notes and mail", isMono: true) {
				TextField("[A-Z]+-\\d+", text: $model.ticketPattern).textFieldStyle(.roundedBorder).textStyle(Tokens.Typography.mono)
			}
			field("Ticket link", key: "ticket_url", note: "Where {ticket} is the id", isMono: true) {
				TextField("https://tracker.example.com/browse/{ticket}", text: $model.ticketLink).textFieldStyle(.roundedBorder).textStyle(Tokens.Typography.mono)
			}
			field("Hosts", key: "hosts", note: "One per line; links to these hosts belong to this workspace", isMono: true) {
				TextEditor(text: $model.hosts)
					.textStyle(Tokens.Typography.mono)
					.frame(height: 72)
					.scrollContentBackground(.hidden)
					.padding(4)
					.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
					.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineControl, lineWidth: 1))
			}
			field("Story template", key: "story_template", note: "A note inside the workspace that new stories start from", isMono: true) {
				TextField("_templates/story.md", text: $model.storyTemplate).textFieldStyle(.roundedBorder).textStyle(Tokens.Typography.mono)
			}
		}
	}

	private var hours: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
			field("Hours a day", key: "hours_per_day", note: "The day plan's daily target") {
				TextField("8", text: $model.hoursPerDay).textFieldStyle(.roundedBorder).frame(width: 80)
			}
			field("Log folder", key: "log_folder", note: "Holds the day logs, inside the workspace folder", isMono: true) {
				TextField("Log", text: $model.logFolder).textFieldStyle(.roundedBorder).textStyle(Tokens.Typography.mono)
			}
			if let closeOut {
				CloseOutFields(closeOut: closeOut, leaveSources: leaveSources)
			}
			if let billing {
				BillingFields(billing: billing)
			} else {
				VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
					Text("Billing").textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.inkMuted)
					Text("Appears once the Toggl and billing feature set is on").textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
				}
			}
		}
	}

	// MARK: Fields

	private func field(_ label: String, key: String, note: String? = nil, isMono: Bool = false, @ViewBuilder control: () -> some View) -> some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			HStack(spacing: 6) {
				Text(label).textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
				Image(systemName: "doc.text").imageScale(.small).foregroundStyle(Tokens.Colors.inkMuted)
					.accessibilityLabel("Saved to settings.json")
			}
			control()
			if let issue = model.issues[key] {
				Label(issue, systemImage: "exclamationmark.circle.fill")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.danger)
					.accessibilityLabel("\(label): \(issue)")
			} else if let note {
				Text(note).textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
	}

	private func hasIssue(in tab: Tab) -> Bool {
		keys(in: tab).contains { model.issues[$0] != nil }
	}

	private var firstTabWithIssue: Tab? {
		Tab.allCases.first(where: hasIssue(in:))
	}

	private func keys(in tab: Tab) -> [String] {
		switch tab {
		case .general: ["name", "accent", "browser"]
		case .folders: []
		case .tasks: ["ticket_pattern", "ticket_url", "hosts", "story_template"]
		case .hours: ["hours_per_day", "log_folder"]
		}
	}
}

/// The week close-out's keys: the log deadline (the close-out runs only
/// with one) and the workspace whose `Leave.md` lists leave and holidays.
private struct CloseOutFields: View {
	@Bindable var closeOut: CloseOutSettingsModel
	let leaveSources: [String]

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
				HStack(spacing: 6) {
					Text("Week close-out").textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
					Image(systemName: "doc.text").imageScale(.small).foregroundStyle(Tokens.Colors.inkMuted)
						.accessibilityLabel("Saved to settings.json")
				}
				Toggle("Log the week by a deadline", isOn: $closeOut.hasDeadline)
				if closeOut.hasDeadline {
					HStack(spacing: Tokens.Spacing.space8) {
						Picker("Deadline day", selection: $closeOut.deadlineDay) {
							ForEach(WeekdayChoice.allCases) { day in
								Text(day.title).tag(day)
							}
						}
						.labelsHidden()
						.frame(width: 90)
						TextField("08:00", text: $closeOut.deadlineTime)
							.textFieldStyle(.roundedBorder)
							.textStyle(Tokens.Typography.mono)
							.frame(width: 70)
							.accessibilityLabel("Deadline time")
					}
				}
				Text("The close-out opens Friday at 14:00 and alerts Monday at 07:30 when something is missing")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
				Text("Leave and holidays").textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
				Picker("Leave source", selection: $closeOut.leaveSource) {
					Text("Life workspace").tag(String?.none)
					ForEach(leaveSources, id: \.self) { name in
						Text(name).tag(String?.some(name))
					}
				}
				.labelsHidden()
				.frame(width: 220)
				Text("Read from Leave.md in that workspace, one date or range per line")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			if let error = closeOut.errorMessage {
				Label(error, systemImage: "exclamationmark.circle.fill")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.danger)
			}
		}
	}
}

/// The billing model of the workspace: written to `settings.json` with only
/// the chosen model's keys; the rate goes to the app's signed store.
private struct BillingFields: View {
	@Bindable var billing: BillingSettingsModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
				HStack(spacing: 6) {
					Text("Billing").textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
					Image(systemName: "doc.text").imageScale(.small).foregroundStyle(Tokens.Colors.inkMuted)
						.accessibilityLabel("Saved to settings.json")
				}
				SegmentedChoice(
					label: "Billing model",
					options: BillingModelChoice.allCases.map { ($0, $0.title) },
					selection: $billing.model,
				)
				.fixedSize()
				Text(billing.model.note).textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
			}
			switch billing.model {
			case .hourly:
				HStack(spacing: Tokens.Spacing.space12) {
					Picker("Round up to", selection: $billing.roundingMinutes) {
						ForEach(BillingSettingsModel.roundingChoices, id: \.self) { minutes in
							Text("\(minutes) min").tag(minutes)
						}
					}
					.frame(width: 200)
					Toggle("Tasks are billable unless marked", isOn: $billing.isBillableByDefault)
				}
				rate
			case .fixed:
				rate
			case .fte:
				HStack(spacing: Tokens.Spacing.space8) {
					Text("Toggl hours a day").foregroundStyle(Tokens.Colors.inkSecondary)
					TextField("8", text: $billing.flatHours).textFieldStyle(.roundedBorder).frame(width: 80)
				}
				.textStyle(Tokens.Typography.callout)
			case .none:
				EmptyView()
			}
			if let error = billing.errorMessage {
				Label(error, systemImage: "exclamationmark.circle.fill")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.danger)
			}
		}
	}

	private var rate: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			HStack(spacing: Tokens.Spacing.space8) {
				Text("Rate per hour").foregroundStyle(Tokens.Colors.inkSecondary)
				TextField("None", text: $billing.rate).textFieldStyle(.roundedBorder).frame(width: 100)
				TextField("EUR", text: $billing.currency).textFieldStyle(.roundedBorder).frame(width: 60).textStyle(Tokens.Typography.mono)
			}
			.textStyle(Tokens.Typography.callout)
			Text("Kept in Vaultday's signed store, never in settings.json or Toggl").textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
		}
	}
}
