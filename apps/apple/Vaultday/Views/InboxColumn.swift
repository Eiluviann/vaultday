import SwiftUI
import VaultdayCore

/// The 300 px Inbox column; triage arrives with mail (PR 12 and 13).
struct InboxColumn: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			Text("Inbox")
				.font(Tokens.Typography.heading.font)
				.foregroundStyle(Tokens.Colors.ink)
			Spacer()
			Text("Nothing to triage")
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.frame(maxWidth: .infinity)
			Spacer()
		}
		.padding(.vertical, 18)
		.padding(.horizontal, 14)
		.frame(maxHeight: .infinity, alignment: .top)
		.background(Tokens.Colors.surface)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.paneFocus(app.focus.focusedPane == .inbox)
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.inbox) }
	}
}
