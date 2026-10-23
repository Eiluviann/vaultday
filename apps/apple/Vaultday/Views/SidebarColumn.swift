import SwiftUI
import VaultdayCore

/// The 220 px sidebar: the workspace switcher's place, and the app state
/// line at the foot. The Now card, rituals and Browse arrive with Home (PR 8).
struct SidebarColumn: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			Text("WORKSPACE")
				.font(Tokens.Typography.eyebrow.font)
				.tracking(Tokens.Typography.eyebrow.tracking)
				.foregroundStyle(Tokens.Colors.inkMuted)
			if let workspace = app.workspace, workspace.phase == .open {
				WorkspaceRow(name: workspace.name, isCurrent: true)
			} else {
				Text("No folder yet")
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			if let problems = app.workspace?.grantProblems, !problems.isEmpty {
				ForEach(problems) { problem in
					Label(problem.reason, systemImage: "exclamationmark.triangle")
						.font(Tokens.Typography.caption.font)
						.foregroundStyle(Tokens.Colors.caution)
				}
			}
			Spacer()
			if let notice = app.unsignedNotice {
				Label(notice, systemImage: "exclamationmark.triangle")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.caution)
			}
			if let error = app.keymapError {
				Label(error, systemImage: "keyboard")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.danger)
			}
			if let status = app.workspace?.appStateStatus, !status.isEmpty {
				Text(status)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.padding(.vertical, Tokens.Spacing.space12)
		.padding(.horizontal, 10)
		.frame(width: HomeLayout.sidebarWidth, alignment: .leading)
		.frame(maxHeight: .infinity, alignment: .top)
		.background(Tokens.Colors.surfaceSidebar)
		.overlay(alignment: .trailing) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.paneFocus(app.focus.focusedPane == .sidebar)
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.sidebar) }
	}
}

private struct WorkspaceRow: View {
	let name: String
	let isCurrent: Bool

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Circle().fill(Tokens.Colors.wsBlue).frame(width: 8, height: 8)
			Text(name)
				.font(Tokens.Typography.headline.font)
				.foregroundStyle(Tokens.Colors.ink)
			Spacer()
		}
		.padding(.horizontal, Tokens.Spacing.space8)
		.frame(height: Tokens.Size.sizeRow)
		.background(isCurrent ? Tokens.Colors.fillControl : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
	}
}

/// The 44 px rail the sidebar collapses into (⌃⌘S): the workspace dot, the
/// app state dot at the foot.
struct SidebarRail: View {
	let app: AppModel

	var body: some View {
		VStack(spacing: 10) {
			Circle()
				.fill(Tokens.Colors.wsBlue)
				.frame(width: 12, height: 12)
				.overlay(Circle().strokeBorder(Tokens.Colors.focus, lineWidth: 2).padding(-3))
				.help(app.workspace?.name ?? "")
			Spacer()
			Circle()
				.fill(app.workspace?.appStateStatus.hasPrefix("App state up to date") == true ? Tokens.Colors.positive : Tokens.Colors.inkDisabled)
				.frame(width: 7, height: 7)
				.help(app.workspace?.appStateStatus ?? "")
		}
		.padding(.vertical, Tokens.Spacing.space12)
		.frame(width: HomeLayout.railWidth)
		.frame(maxHeight: .infinity)
		.background(Tokens.Colors.surfaceSidebar)
		.overlay(alignment: .trailing) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.paneFocus(app.focus.focusedPane == .sidebar)
	}
}
