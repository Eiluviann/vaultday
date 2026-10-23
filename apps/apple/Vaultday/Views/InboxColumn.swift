import SwiftUI
import VaultdayCore

/// The 300 px Inbox column: the signals still in triage as cards, newest
/// first, "Triage all" for the full screen, and the empty state ("Connect a
/// mail account" until one exists, then "No new mail"). While anything is
/// dragged over it the whole pane becomes the drop placeholder.
struct InboxColumn: View {
	let app: AppModel
	@State private var isTargeted = false

	var body: some View {
		let inbox = app.inbox
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space8) {
				Text("Inbox")
					.font(Tokens.Typography.heading.font)
					.foregroundStyle(Tokens.Colors.ink)
				if !inbox.cards.isEmpty {
					Text("\(inbox.cards.count) to decide")
						.font(Tokens.Typography.caption.font)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
				Spacer()
				if inbox.isOpen, !inbox.cards.isEmpty || !inbox.snoozed.isEmpty {
					Button("Triage all") { app.showTriage() }
						.buttonStyle(.plain)
						.font(Tokens.Typography.callout.font.weight(.semibold))
						.foregroundStyle(Tokens.Colors.link)
						.help("Open the full inbox")
				}
			}
			if let message = inbox.message {
				InboxMessage(text: message)
			}
			if inbox.cards.isEmpty {
				InboxEmptyState(hasMailAccount: app.hasMailAccount)
					.padding(.top, Tokens.Spacing.space4)
				Spacer()
			} else {
				SignalList(app: app, cards: inbox.cards)
			}
		}
		.padding(.vertical, 18)
		.padding(.horizontal, 14)
		.frame(maxHeight: .infinity, alignment: .top)
		.paneGround(Tokens.Colors.surface, isFocused: app.focus.focusedPane == .inbox)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.overlay {
			if isTargeted {
				InboxDropPlaceholder()
			}
		}
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.inbox) }
		// A signal dragged out and let go here stays in the Inbox; nothing else lands here.
		.dropDestination(for: String.self) { payloads, _ in
			guard let payload = payloads.first, case .signal = DragPayload(payload) else { return false }
			return true
		} isTargeted: { isTargeted = $0 }
	}
}

/// The scrolling list of signal cards, shared by the column and the triage screen.
struct SignalList: View {
	let app: AppModel
	let cards: [SignalCard]
	/// The cards there at first render; the rest arrived while on screen.
	@State private var arrivals = ArrivalLedger()

	var body: some View {
		ScrollView {
			LazyVStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				ForEach(Array(cards.enumerated()), id: \.element.id) { index, card in
					SignalCardView(
						app: app,
						card: card,
						isSelected: app.focus.focusedPane == .inbox && app.focus.selection(in: .inbox) == index,
						isArrival: arrivals.isArrival(card.id),
					)
				}
			}
			.padding(.vertical, Tokens.Spacing.space4)
		}
		.scrollIndicators(.automatic)
		.onAppear { arrivals.settle(cards.map(\.id)) }
		.onChange(of: cards.map(\.id)) { _, ids in arrivals.record(ids) }
	}
}

/// The whole pane as the drop placeholder while something is dragged over it.
private struct InboxDropPlaceholder: View {
	var body: some View {
		RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg)
			.fill(Tokens.Colors.surfaceSunken.opacity(0.92))
			.overlay(
				RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg)
					.strokeBorder(Tokens.Colors.focus, style: StrokeStyle(lineWidth: 1.5, dash: [5, 4])),
			)
			.overlay {
				Label("Let go to leave it in the Inbox", systemImage: "tray")
					.font(Tokens.Typography.callout.font.weight(.semibold))
					.foregroundStyle(Tokens.Colors.inkSecondary)
			}
			.padding(8)
			.allowsHitTesting(false)
			.accessibilityHidden(true)
	}
}

/// What the last outcome wrote, or why it did not run.
struct InboxMessage: View {
	let text: String

	var body: some View {
		Text(text)
			.font(Tokens.Typography.caption.font)
			.foregroundStyle(Tokens.Colors.inkSecondary)
			.lineLimit(2)
			.accessibilityAddTraits(.updatesFrequently)
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
