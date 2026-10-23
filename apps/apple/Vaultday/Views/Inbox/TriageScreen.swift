import SwiftUI
import VaultdayCore

/// "Triage all": the full Inbox in place of Today, the Inbox column and the
/// side panel. The cards take the keys of the Inbox pane (`1` + Task, `2`
/// Wait for, `3` Attach, `4` Dismiss, `>>` Snooze, `\` the menu); the aside
/// lists what is snoozed and where the mail comes from.
struct TriageScreen: View {
	let app: AppModel
	@State private var tab: TriageTab = .new

	private enum TriageTab: Hashable {
		case new, snoozed
	}

	var body: some View {
		let inbox = app.inbox
		HStack(spacing: 0) {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
				HStack(spacing: Tokens.Spacing.space8) {
					Button {
						app.leaveTriage()
					} label: {
						Label("Day", systemImage: "chevron.left")
					}
					.buttonStyle(.plain)
					.foregroundStyle(Tokens.Colors.link)
					.help("Back to the day")
					Text("Inbox")
						.font(Tokens.Typography.titleLarge.font)
						.foregroundStyle(Tokens.Colors.ink)
					Text("\(inbox.cards.count) to decide · \(inbox.snoozed.count) snoozed")
						.font(Tokens.Typography.callout.font)
						.foregroundStyle(Tokens.Colors.inkMuted)
					Spacer()
				}
				Picker("Show", selection: $tab) {
					Text("New \(inbox.cards.count)").tag(TriageTab.new)
					Text("Snoozed \(inbox.snoozed.count)").tag(TriageTab.snoozed)
				}
				.pickerStyle(.segmented)
				.labelsHidden()
				.frame(width: 260)
				if let message = inbox.message {
					InboxMessage(text: message)
				}
				switch tab {
				case .new:
					if inbox.cards.isEmpty {
						ContentUnavailableView("Nothing to decide", systemImage: "checkmark.circle", description: Text("New mail lands here after the next fetch"))
					} else {
						SignalList(app: app, cards: inbox.cards)
							.frame(maxWidth: 640)
					}
				case .snoozed:
					SnoozedList(cards: inbox.snoozed)
				}
				Text(keyLine)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			.padding(18)
			.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
			.paneGround(Tokens.Colors.surface, isFocused: app.focus.focusedPane == .inbox)
			.contentShape(Rectangle())
			.onTapGesture { app.focus.focus(.inbox) }
			TriageAside(app: app)
				.frame(width: HomeLayout.sidePanelWidth)
		}
	}

	/// The keys of the Inbox pane, from the keymap.
	private var keyLine: String {
		let keys: [(KeyAction, String)] = [
			(.signalTask, "+ Task"), (.signalWaitFor, "Wait for"), (.signalAttach, "Attach"),
			(.signalDismiss, "Dismiss"), (.signalSnooze, "Snooze"), (.openItemMenu, "More"),
		]
		let parts = keys.compactMap { action, name in app.keyLabel(action).map { "\($0) \(name)" } }
		return (parts + ["outcomes write one vault line, Snooze, Attach and Dismiss none"]).joined(separator: " · ")
	}
}

/// The snoozed signals, soonest back first.
private struct SnoozedList: View {
	let cards: [SignalCard]

	var body: some View {
		if cards.isEmpty {
			ContentUnavailableView("Nothing snoozed", systemImage: "moon.zzz")
		} else {
			ScrollView {
				LazyVStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
					ForEach(cards) { card in
						SnoozedRow(card: card)
					}
				}
			}
			.frame(maxWidth: 640)
		}
	}
}

private struct SnoozedRow: View {
	let card: SignalCard

	var body: some View {
		HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space8) {
			Image(systemName: card.source.mark.symbol)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.accessibilityLabel(card.sourceName)
			VStack(alignment: .leading, spacing: 2) {
				Text(card.headline).foregroundStyle(Tokens.Colors.ink)
				Text(card.sender).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
			}
			Spacer()
			Text(until)
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
		}
		.font(Tokens.Typography.callout.font)
		.padding(10)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd).strokeBorder(Tokens.Colors.line))
	}

	private var until: String {
		guard case let .snoozed(date) = card.state, let date else { return "snoozed" }
		return "until \(date.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated).hour().minute()))"
	}
}

/// The triage screen's aside: where the mail comes from and when it was last read.
private struct TriageAside: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			HomeEyebrow(text: "Sources")
			if let scheduler = app.mailScheduler {
				if let fetch = scheduler.lastFetch {
					Text("Mail read \(fetch.finishedAt.formatted(date: .omitted, time: .shortened)) · \(fetch.added) new")
						.foregroundStyle(Tokens.Colors.inkSecondary)
					ForEach(fetch.failures, id: \.self) { failure in
						Label(failure, systemImage: "exclamationmark.triangle")
							.foregroundStyle(Tokens.Colors.caution)
					}
				} else {
					Text(app.hasMailAccount ? "Not read yet" : "Connect a mail account")
						.foregroundStyle(Tokens.Colors.inkSecondary)
				}
				Button(scheduler.isFetching ? "Reading mail" : "Read mail now") {
					Task { await scheduler.fetchNow() }
				}
				.disabled(scheduler.isFetching || !app.hasMailAccount)
			}
			HomeEyebrow(text: "Lanes")
			PlannedLane(symbol: "figure.run", title: "Chasers", detail: "Replies you wait on resurface here from Sent Items") // custom symbol pending: running figure
			PlannedLane(symbol: "line.3.horizontal.decrease.circle", title: "Screener", detail: "First mail from a new sender waits here before the Inbox")
			Text("Notifications are quiet: one per fetch that brings new mail, never one per message")
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
			Spacer()
		}
		.font(Tokens.Typography.callout.font)
		.padding(18)
		.frame(maxHeight: .infinity, alignment: .topLeading)
		.background(Tokens.Colors.surfaceSunken)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
	}
}

/// A lane of the triage screen the base version does not run yet: named,
/// tagged Planned, never empty space.
private struct PlannedLane: View {
	let symbol: String
	let title: String
	let detail: String

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
			HStack(spacing: Tokens.Spacing.space6) {
				Image(systemName: symbol)
					.frame(width: 16)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.accessibilityHidden(true)
				Text(title)
					.fontWeight(.semibold)
					.foregroundStyle(Tokens.Colors.ink)
				PlannedTag()
			}
			Text(detail)
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.fixedSize(horizontal: false, vertical: true)
				.padding(.leading, 22)
		}
		.accessibilityElement(children: .combine)
		.accessibilityLabel("\(title), planned")
	}
}
