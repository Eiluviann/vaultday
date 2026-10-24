import SwiftUI
import VaultdayCore

/// The All view (⌘ and the key left of 1): every workspace's day side by side under its own
/// header, read-only. Tasks are ticked, moved and added in their workspace.
struct AllWorkspacesColumn: View {
	let app: AppModel
	let all: AllWorkspacesModel

	var body: some View {
		VStack(alignment: .leading, spacing: 10) {
			DayHeader(title: all.title, isToday: all.isToday, app: app, showsScheduling: false)
			HStack(alignment: .firstTextBaseline, spacing: 14) {
				Text("Everything")
					.textStyle(Tokens.Typography.heading)
					.foregroundStyle(Tokens.Colors.ink)
				Text("Read-only; open a workspace to change its plan")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
				Spacer()
			}
			ScrollView {
				LazyVStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
					ForEach(all.groups) { group in
						GroupHeader(group: group)
							.padding(.top, group.id == all.groups.first?.id ? 6 : 8)
						if let problem = group.problem {
							Label(problem, systemImage: "exclamationmark.triangle")
								.textStyle(Tokens.Typography.caption)
								.foregroundStyle(Tokens.Colors.caution)
						} else if group.items.isEmpty {
							Text("Nothing planned")
								.textStyle(Tokens.Typography.callout)
								.foregroundStyle(Tokens.Colors.inkMuted)
								.padding(.leading, 18)
						}
						ForEach(group.items) { item in
							TaskCard(app: app, item: item, isSelected: false, readOnlyColor: group.color)
						}
					}
				}
				.padding(.vertical, Tokens.Spacing.space4)
			}
		}
		.padding(EdgeInsets(top: 18, leading: 22, bottom: 18, trailing: 22))
		.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
		.paneGround(Tokens.Colors.surface, isFocused: app.focus.focusedPane == .today)
		.environment(\.workspaceAccent, Tokens.Colors.wsGraphite)
	}
}

/// A workspace's dot, its name in its color, and its planned time and waiting count.
private struct GroupHeader: View {
	let group: WorkspaceGroup

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Circle()
				.fill(group.color)
				.frame(width: 10, height: 10)
			Text(group.name.uppercased())
				.textStyle(Tokens.Typography.eyebrow)
				.foregroundStyle(group.color)
			Text(summary)
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
		.accessibilityElement(children: .combine)
		.accessibilityAddTraits(.isHeader)
	}

	private var summary: String {
		var parts = [DurationText.minutes(group.plannedMinutes)]
		if group.waitingCount > 0 { parts.append("\(group.waitingCount) waiting") }
		return parts.joined(separator: " · ")
	}
}
