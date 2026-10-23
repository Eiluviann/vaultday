import SwiftUI
import VaultdayCore

/// Step 12: ready. What the setup chose, sphere by sphere; the footer's
/// Open Vaultday hands over to the main window.
struct ReadyStep: View {
	let workspaces: WorkspacesModel
	let ready: ReadyStepModel

	var body: some View {
		StepPane(title: "Vaultday is ready", lede: "Your notes, reminders, calendars and mail are in one place now, sorted into Work and Life. Everything here can be changed in Settings") {
			SetupCallout(icon: "icloud", text: ready.syncLine)
			ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
				SpherePanel(sphere: sphere) { EmptyView() } content: {
					let lines = ready.lines[sphere] ?? []
					if lines.isEmpty {
						Text(sphere == .work ? "No work workspace yet" : "No life workspace yet")
							.font(Tokens.Typography.callout.font)
							.foregroundStyle(Tokens.Colors.inkMuted)
							.padding(.vertical, Tokens.Spacing.space8)
					}
					ForEach(lines) { line in
						SetupCard(icon: line.icon, iconTone: sphere == .work ? .work : .life, title: line.title, description: line.detail) { EmptyView() }
					}
				}
			}
			SetupCard(icon: "terminal", title: "The vaultday command", description: "Arrives with the timer", tags: [SetupTag(text: "Planned")]) { EmptyView() }
			SetupCard(icon: "menubar.rectangle", title: "Menu bar and Quick Entry", description: "Arrives with the timer", tags: [SetupTag(text: "Planned")]) { EmptyView() }
		}
		.onAppear { ready.refresh(workspaces: workspaces.entries) }
	}
}
