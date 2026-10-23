import SwiftUI
import VaultdayCore

/// The signal preview slide-over: the sender, time and id, the headline, the
/// excerpt the store keeps, and the outcomes. It slides over the Inbox and
/// the side panel from the right edge; Esc or ✕ closes it.
struct SignalPreviewPanel: View {
	let app: AppModel
	let card: SignalCard
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		VStack(alignment: .leading, spacing: 12) {
			HStack(spacing: 10) {
				Button {
					app.inbox.closePreview()
				} label: {
					Image(systemName: "xmark")
						.font(.system(size: 10, weight: .semibold))
						.frame(width: 28, height: 24)
						.foregroundStyle(Tokens.Colors.inkSecondary)
						.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
						.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
				}
				.buttonStyle(.plain)
				.accessibilityLabel("Close preview")
				Image(systemName: card.source.mark.symbol)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.accessibilityLabel(card.sourceName)
				Text("\(card.sender) · \(card.timeLabel)")
					.font(Tokens.Typography.callout.font.weight(.semibold))
					.foregroundStyle(Tokens.Colors.ink)
					.lineLimit(1)
				Spacer(minLength: 8)
				MonoChip(text: card.id, isRounded: false)
				Button("Reply in the mail app") { _ = app.inbox.replyInMailApp(signalId: card.id) }
					.buttonStyle(.plain)
					.font(Tokens.Typography.callout.font.weight(.semibold))
					.foregroundStyle(Tokens.Colors.link)
			}
			Text(card.headline)
				.font(Tokens.Typography.title.font)
				.foregroundStyle(Tokens.Colors.ink)
				.fixedSize(horizontal: false, vertical: true)
			VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
				fact("From", card.senderAddress)
				fact("Subject", card.subject)
				if let ticket = card.ticket {
					fact("Ticket", card.ticketStoryTitle.map { "\(ticket) · \($0)" } ?? "\(ticket) · no story in this workspace")
				}
			}
			Text("\u{201C}\(card.excerpt)\u{201D}")
				.font(Tokens.Typography.body.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.fixedSize(horizontal: false, vertical: true)
			Text("Excerpt only; the message stays in the mail account and its text in the encrypted store")
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
			if card.state.isInTriage {
				HStack(spacing: Tokens.Spacing.space6) {
					Button {
						outcome(.task)
					} label: {
						HStack(spacing: 5) {
							Image(systemName: "plus").font(.system(size: 9, weight: .semibold))
							Text("Task")
						}
						.fontWeight(.semibold)
						.foregroundStyle(Tokens.Colors.onWs)
						.padding(.horizontal, 10)
						.frame(height: 26)
						.background(accent, in: RoundedRectangle(cornerRadius: 7))
					}
					.buttonStyle(.plain)
					OutlineButton(title: card.waitForLabel, cap: .signalWaitFor, app: app) { outcome(.waitFor) }
					OutlineButton(title: "Attach", cap: .signalAttach, app: app) { outcome(.attach) }
					OutlineButton(title: "Snooze", cap: .signalSnooze, app: app) { outcome(.snooze) }
					OutlineButton(title: "Dismiss", cap: .signalDismiss, app: app) {
						app.inbox.closePreview()
						app.inbox.dismiss(signalId: card.id)
					}
				}
				Text("Dismissed and snoozed signals write nothing to the vault")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			Spacer(minLength: 0)
		}
		.font(Tokens.Typography.callout.font)
		.padding(.horizontal, 22)
		.padding(.vertical, 18)
		.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
		.background(Tokens.Colors.surfaceRaised)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.shadow(color: .black.opacity(0.08), radius: 12, x: -6)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Signal preview")
	}

	private func fact(_ label: String, _ value: String) -> some View {
		HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space8) {
			Text(label)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.frame(width: 62, alignment: .leading)
			Text(value)
				.foregroundStyle(Tokens.Colors.ink)
				.textSelection(.enabled)
		}
	}

	/// The outcome popover opens on the card, back in the list.
	private func outcome(_ kind: SignalDraftKind) {
		app.select(signal: card)
		app.inbox.closePreview()
		app.inbox.begin(kind, signalId: card.id)
	}
}
