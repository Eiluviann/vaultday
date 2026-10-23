import SwiftUI
import VaultdayCore

/// A signal card, as the design note draws it: the source icon and the
/// sender on the first line with the time alone on the right, the headline
/// below, a tag row with the ticket number, the two primary outcomes with the
/// more-actions button, and the secondary text actions. Nothing on it is guessed.
struct SignalCardView: View {
	let app: AppModel
	let card: SignalCard
	let isSelected: Bool
	/// The card arrived while the Inbox was on screen: its mark draws on.
	var isArrival = false

	@State private var isHovered = false
	@State private var hasAppeared = false
	@Environment(\.workspaceAccent) private var accent

	private var draft: SignalDraft? {
		app.inbox.draft.flatMap { $0.signalId == card.id ? $0 : nil }
	}

	var body: some View {
		VStack(alignment: .leading, spacing: 5) {
			senderLine
			Button {
				app.select(signal: card)
				app.inbox.openPreview(signalId: card.id)
			} label: {
				Text(card.headline)
					.fontWeight(.semibold)
					.foregroundStyle(Tokens.Colors.ink)
					.multilineTextAlignment(.leading)
					.frame(maxWidth: .infinity, alignment: .leading)
					.fixedSize(horizontal: false, vertical: true)
			}
			.buttonStyle(.plain)
			.help("Open the preview")
			if let ticket = card.ticket {
				HStack(spacing: Tokens.Spacing.space4) {
					MonoChip(text: ticket)
						.help(card.ticketStoryTitle.map { "\(ticket) · \($0)" } ?? "No story \(ticket) in this workspace")
				}
			}
			primaryRow
			if draft != nil {
				SignalDraftPopover(app: app, inbox: app.inbox)
			}
			secondaryRow
		}
		.font(Tokens.Typography.callout.font)
		.padding(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 12))
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 10))
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(isSelected ? accent : Tokens.Colors.line, lineWidth: isSelected ? 2 : 1))
		.lift(isHovered || isSelected)
		.contentShape(Rectangle())
		.onHover { hovering in
			isHovered = hovering
			if hovering { app.hover(signal: card) }
		}
		.onTapGesture(count: 2) { app.inbox.openPreview(signalId: card.id) }
		.onTapGesture { app.select(signal: card) }
		.draggable(card.id)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("\(card.sender), \(card.headline)")
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}

	private var senderLine: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			// A signal that arrives while the Inbox is on screen draws its
			// mark on; the cards there at first render show theirs at once.
			IconPresence(isShown: !isArrival || hasAppeared) {
				MarkIcon(mark: card.source.mark)
			}
			.frame(width: 16, height: 16)
			.onAppear { hasAppeared = true }
			Text(card.sender)
				.fontWeight(.semibold)
				.foregroundStyle(Tokens.Colors.ink)
				.lineLimit(1)
				.help(card.senderAddress)
			Spacer(minLength: Tokens.Spacing.space4)
			Text(card.timeLabel)
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.monospacedDigit()
		}
	}

	private var primaryRow: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			Button {
				begin(.task)
			} label: {
				HStack(spacing: 5) {
					Image(systemName: "plus").font(.system(size: 9, weight: .semibold))
					Text("Task")
					ActionCap(app: app, action: .signalTask, onFill: Tokens.Colors.onWs)
				}
				.fontWeight(.semibold)
				.foregroundStyle(Tokens.Colors.onWs)
				.padding(.horizontal, 10)
				.frame(height: 24)
				.background(accent, in: RoundedRectangle(cornerRadius: 6))
			}
			.buttonStyle(.plain)
			.help("+ Task: a line under Next Steps of the matched story, or the Mail note")
			OutlineButton(title: card.waitForLabel, cap: .signalWaitFor, app: app) { begin(.waitFor) }
				.help("Wait for: a line under Pending Input with the sender as assigned_to")
			Spacer(minLength: 0)
			Button {
				app.select(signal: card)
				app.perform(.openItemMenu)
			} label: {
				HStack(spacing: 6) {
					Image(systemName: "ellipsis").font(.system(size: 12))
					ActionCap(app: app, action: .openItemMenu)
				}
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.padding(.horizontal, 6)
				.frame(minWidth: 28, minHeight: 24)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 6))
				.overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Tokens.Colors.lineButton))
			}
			.buttonStyle(.plain)
			.help("More actions")
			.accessibilityLabel("More actions")
			.overlay(alignment: .topTrailing) {
				if isSelected, app.keyboard.showsItemMenu(in: .inbox) {
					ItemMenuPanel(app: app, isForSignal: true)
						.offset(y: 28)
						.zIndex(1)
				}
			}
		}
		.zIndex(1)
	}

	private var secondaryRow: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			TextAction(title: "Snooze", cap: .signalSnooze, app: app) { begin(.snooze) }
				.help("Hides the signal until a date")
			TextAction(title: "Dismiss", cap: .signalDismiss, app: app) {
				app.select(signal: card)
				app.inbox.dismiss(signalId: card.id)
			}
			.help("Dismiss: writes nothing to the vault")
			Spacer(minLength: 0)
			Button("Reply") { _ = app.inbox.replyInMailApp(signalId: card.id) }
				.buttonStyle(.plain)
				.font(Tokens.Typography.callout.font.weight(.semibold))
				.foregroundStyle(Tokens.Colors.link)
				.help("Reply in the mail app")
		}
	}

	private func begin(_ kind: SignalDraftKind) {
		app.select(signal: card)
		app.inbox.begin(kind, signalId: card.id)
	}
}

/// A bordered secondary button with its key cap.
struct OutlineButton: View {
	let title: String
	let cap: KeyAction
	let app: AppModel
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: 5) {
				Text(title).lineLimit(1)
				ActionCap(app: app, action: cap)
			}
			.foregroundStyle(Tokens.Colors.ink)
			.padding(.horizontal, 10)
			.frame(height: 24)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 6))
			.overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Tokens.Colors.lineButton))
		}
		.buttonStyle(.plain)
	}
}

/// A borderless muted text action with its key cap.
struct TextAction: View {
	let title: String
	let cap: KeyAction
	let app: AppModel
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: 5) {
				Text(title)
				ActionCap(app: app, action: cap)
			}
			.foregroundStyle(Tokens.Colors.inkMuted)
			.frame(height: 24)
		}
		.buttonStyle(.plain)
	}
}
