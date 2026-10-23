import SwiftUI
import VaultdayCore

/// Step 5: accounts. Mail accounts connect in a later release, so the step is
/// drawn, tagged Planned and inert; what each workspace wants from its
/// accounts is stored for then.
struct AccountsStep: View {
	let workspaces: WorkspacesModel
	let accounts: AccountsStepModel

	private let parts: [(AccountPartChoice, String, String)] = [
		(.mail, "envelope", "flagged and unread become inbox items"),
		(.calendar, "calendar", "events your day is planned around"),
		(.contacts, "person.2", "people on your tickets and mail"),
		(.reminders, "checklist", "items you add become tasks"),
	]

	var body: some View {
		StepPane(title: "Your accounts", lede: "Add the accounts you use for mail, calendars and reminders to a workspace, then pick what Vaultday brings in from each") {
			SetupCallout(icon: "info.circle", text: AccountsStepModel.plannedNote) { PlannedTag() }
			if workspaces.entries.isEmpty {
				SetupCallout(icon: "folder.badge.plus", text: "Add a workspace first; its accounts are listed here")
			}
			LazyVGrid(columns: [GridItem(.adaptive(minimum: 280), spacing: Tokens.Spacing.space16, alignment: .top)], alignment: .leading, spacing: Tokens.Spacing.space16) {
				ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
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
	}

	private func workspaceCard(_ entry: WorkspaceEntry) -> some View {
		let number = (workspaces.entries.firstIndex(of: entry) ?? 0) + 1
		return VStack(spacing: 0) {
			HStack(spacing: Tokens.Spacing.space8) {
				WorkspaceBadge(number: number, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere))
				Text(entry.name).font(Tokens.Typography.headline.font).foregroundStyle(Tokens.Colors.ink)
				SetupTag(text: "Active", tone: .on, icon: "checkmark")
				Spacer()
				PlannedTag()
				Button {} label: { SetupLabel(title: "Add", icon: "plus") }
					.buttonStyle(SetupButtonStyle(isSmall: true))
					.disabled(!accounts.canAddAccounts)
					.accessibilityLabel("Add an account to \(entry.name)")
			}
			.frame(minHeight: Tokens.Size.sizeRow)
			.padding(Tokens.Spacing.space12)
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
}
