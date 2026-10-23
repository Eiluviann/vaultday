import SwiftUI
import VaultdayCore

/// The 300 px Inbox column. Mail is fetched into the signal store since
/// PR 12; the signal cards themselves arrive with PR 13, so the column holds
/// its empty state: "Connect a mail account" until one exists, then "No new mail".
struct InboxColumn: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			Text("Inbox")
				.font(Tokens.Typography.heading.font)
				.foregroundStyle(Tokens.Colors.ink)
			InboxEmptyState(hasMailAccount: app.hasMailAccount)
				.padding(.top, Tokens.Spacing.space4)
			Spacer()
		}
		.padding(.vertical, 18)
		.padding(.horizontal, 14)
		.frame(maxHeight: .infinity, alignment: .top)
		.paneGround(Tokens.Colors.surface, isFocused: app.focus.focusedPane == .inbox)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.inbox) }
	}
}

/// The empty column, as the rows board draws it: a dashed card with a glyph and one line.
private struct InboxEmptyState: View {
	let hasMailAccount: Bool

	var body: some View {
		VStack(spacing: Tokens.Spacing.space6) {
			Image(systemName: hasMailAccount ? "checkmark.circle" : "envelope")
				.font(.system(size: 16))
				.foregroundStyle(hasMailAccount ? Tokens.Colors.positive : Tokens.Colors.inkMuted)
			Text(hasMailAccount ? "No new mail" : "Connect a mail account")
				.fontWeight(.semibold)
				.foregroundStyle(Tokens.Colors.inkSecondary)
		}
		.font(Tokens.Typography.callout.font)
		.padding(EdgeInsets(top: 18, leading: 16, bottom: 18, trailing: 16))
		.frame(maxWidth: .infinity)
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Tokens.Colors.lineControl, style: StrokeStyle(lineWidth: 1, dash: [4, 3])))
		.accessibilityElement(children: .combine)
	}
}
