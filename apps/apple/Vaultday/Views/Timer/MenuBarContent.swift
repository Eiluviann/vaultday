import SwiftUI
import VaultdayCore

/// The menu bar item's title: the porthole, and the running clock beside it.
struct MenuBarLabel: View {
	let app: AppModel

	var body: some View {
		if let clock = app.menuBarClock {
			Label(clock, systemImage: app.runningSession?.isPaused == true ? "pause.circle" : "timer")
				.labelStyle(.titleAndIcon)
				.iconReplace(value: app.runningSession?.isPaused == true)
				.monospacedDigit()
		} else {
			Image("MenuBarIcon")
				.accessibilityLabel("Vaultday")
		}
	}
}

/// The menu-bar extra, Timery style: the day's logged ring, the current
/// entry with its elapsed time and Stop or Edit, then recent tasks to start
/// with ↑ ↓ and ⏎, and ⌘N for a typed new session.
struct MenuBarContent: View {
	let app: AppModel
	@FocusState private var isListFocused: Bool
	@FocusState private var isDraftFocused: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			LoggedHeader(app: app)
			if let session = app.runningSession {
				CurrentEntry(app: app, session: session)
			} else {
				idleActions
			}
			if app.timerPicker.isComposing {
				newEntryField
			}
			HomeEyebrowText(text: app.workspace.map { "Recent · \($0.name)" } ?? "Recent")
				.padding(EdgeInsets(top: 8, leading: 10, bottom: 2, trailing: 10))
			if app.timerPicker.picks.isEmpty {
				Text("No open tasks today")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.padding(.horizontal, 10)
			}
			ForEach(Array(app.timerPicker.picks.enumerated()), id: \.element.id) { index, pick in
				PickRow(pick: pick, isSelected: index == app.timerPicker.selectedIndex && !app.timerPicker.isComposing) {
					app.timerPicker.select(pick)
					app.startPickedTimer()
				}
			}
			if let message = app.timerMessage {
				Label(message, systemImage: "exclamationmark.triangle")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.danger)
					.padding(.horizontal, 10)
			}
			footer
			Text("Long-running timer alert after \(DurationText.minutes(app.rhythm?.longTimerCheckMinutes ?? 180))")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.padding(EdgeInsets(top: 0, leading: 10, bottom: 4, trailing: 10))
		}
		.padding(10)
		.frame(width: 330)
		.background(Tokens.Colors.surface)
		.foregroundStyle(Tokens.Colors.ink)
		.textStyle(Tokens.Typography.body)
		// The popover's window stays out of a screen share while the mode is on,
		// so it keeps its values: no screen-share environment here.
		.background(WindowReader { app.windowSharing.register($0) })
		.focusable()
		.focusEffectDisabled()
		.focused($isListFocused)
		.onAppear {
			app.refreshTimerPicker()
			isListFocused = true
		}
		// The keys come from Keymap.yaml's `menu-bar` bindings.
		.onKeyPress(phases: .down) { press in
			guard let action = app.menuBarAction(for: KeyStroke(press)) else { return .ignored }
			return perform(action) ? .handled : .ignored
		}
	}

	private func perform(_ action: KeyAction) -> Bool {
		switch action {
		case .menuBarPrevious:
			app.timerPicker.moveSelection(by: -1)
		case .menuBarNext:
			app.timerPicker.moveSelection(by: 1)
		case .menuBarStart:
			app.startPickedTimer()
		case .menuBarNewEntry:
			app.timerPicker.beginNewEntry()
			isDraftFocused = true
		case .menuBarCancel:
			guard app.timerPicker.isComposing else { return false }
			app.timerPicker.endNewEntry()
			isListFocused = true
		default:
			return false
		}
		return true
	}

	private var idleActions: some View {
		VStack(alignment: .leading, spacing: 0) {
			if let latest = app.sessions.latestTimerSession {
				ActionRow(symbol: "arrow.clockwise", title: "Continue latest · \(app.sessionText(latest))") {
					app.continueLatestTimer()
				}
			}
			HStack(spacing: 10) {
				Image(systemName: "clock")
					.iconGlyph(.row)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.frame(width: 14)
				Text("Start minutes ago")
				Spacer()
				ForEach([5, 10, 30], id: \.self) { minutes in
					Button("\(minutes)") { app.startPickedTimer(minutesAgo: minutes) }
						.buttonStyle(.borderless)
						.textStyle(Tokens.Typography.caption)
						.help("Start the selected task \(minutes) minutes ago")
						.disabled(app.timerPicker.selectedPick == nil && !app.timerPicker.isComposing)
				}
			}
			.padding(EdgeInsets(top: 7, leading: 10, bottom: 7, trailing: 10))
		}
	}

	private var newEntryField: some View {
		TextField("Task id, story or words of a task", text: Binding(get: { app.timerPicker.draft }, set: { app.timerPicker.draft = $0 }))
			.textFieldStyle(.roundedBorder)
			.focused($isDraftFocused)
			.onSubmit { app.startPickedTimer() }
			.padding(.horizontal, 10)
			.accessibilityLabel("New session")
	}

	private var footer: some View {
		HStack(spacing: Tokens.Spacing.space12) {
			KeyHint(keys: "⌘N", text: "new session")
			KeyHint(keys: "↑↓", text: "move")
			KeyHint(keys: "⏎", text: "start")
			Spacer()
			Button("Open Vaultday") { app.openMainWindow() }
				.buttonStyle(.borderless)
		}
		.textStyle(Tokens.Typography.caption)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.padding(EdgeInsets(top: 8, leading: 10, bottom: 4, trailing: 10))
		.overlay(alignment: .top) { Rectangle().fill(Tokens.Colors.lineFaint).frame(height: 1) }
		.padding(.top, Tokens.Spacing.space4)
	}
}

private struct LoggedHeader: View {
	let app: AppModel

	var body: some View {
		HStack(spacing: Tokens.Spacing.space12) {
			ZStack {
				Circle().stroke(Tokens.Colors.line, lineWidth: 4)
				Circle()
					.trim(from: 0, to: app.loggedFraction)
					.stroke(app.accentColor, style: StrokeStyle(lineWidth: 4, lineCap: .round))
					.rotationEffect(.degrees(-90))
					.progressDraw(value: app.loggedFraction)
			}
			.frame(width: 36, height: 36)
			.padding(4)
			.accessibilityLabel(app.loggedSummary)
			VStack(alignment: .leading, spacing: 2) {
				Text(app.loggedSummary)
					.fontWeight(.bold)
				HStack(spacing: 4) {
					if let name = app.workspace?.name { Text(name + " ·") }
					PrivateTimeValue("\(DurationText.minutes(app.dayPlan.totals.spentMinutes)) tracked today", carriesHand: true)
				}
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
			}
			Spacer()
		}
		.padding(EdgeInsets(top: 8, leading: 10, bottom: 8, trailing: 10))
	}
}

private struct CurrentEntry: View {
	let app: AppModel
	let session: SessionSummary

	var body: some View {
		VStack(alignment: .leading, spacing: 6) {
			Text([session.story, session.taskId].compactMap(\.self).joined(separator: " · "))
				.textStyle(Tokens.Typography.caption)
				.opacity(0.85)
			Text(app.sessionText(session))
				.fontWeight(.semibold)
				.lineLimit(2)
			HStack(spacing: Tokens.Spacing.space8) {
				Text(DurationText.clock(seconds: session.elapsed(at: app.timerClock)))
					.textStyle(Tokens.Typography.timer)
					.monospacedDigit()
				Spacer()
				EntryButton(title: session.isPaused ? "Resume" : "Pause") { app.togglePauseTimer() }
				EntryButton(title: "Stop") { app.stopTimer() }
					.fontWeight(.semibold)
				EntryButton(title: "Edit") { app.openMainWindow(taskId: session.taskId) }
					.help("Open the task in Vaultday")
			}
		}
		.foregroundStyle(Tokens.Colors.onWs)
		.padding(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(app.accentColor, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
	}
}

private struct EntryButton: View {
	let title: String
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			Text(title)
				.padding(.horizontal, 10)
				.frame(height: 26)
				.background(Tokens.Colors.onWs.opacity(0.22), in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
		}
		.buttonStyle(.plain)
	}
}

private struct PickRow: View {
	let pick: TimerPick
	let isSelected: Bool
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: 10) {
				Text(pick.taskId)
					.font(Tokens.Typography.caption.font.monospaced())
					.foregroundStyle(Tokens.Colors.inkMuted)
				Text(pick.text)
					.lineLimit(1)
				Spacer()
				if isSelected {
					Text("⏎")
						.textStyle(Tokens.Typography.kbd)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
			}
			.padding(EdgeInsets(top: 7, leading: 10, bottom: 7, trailing: 10))
			.focusBorder(isSelected, cornerRadius: Tokens.Radius.radiusSm)
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
		.accessibilityLabel("Start \(pick.text)")
	}
}

private struct ActionRow: View {
	let symbol: String
	let title: String
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: 10) {
				Image(systemName: symbol)
					.iconGlyph(.row)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.frame(width: 14)
				Text(title).lineLimit(1)
				Spacer()
			}
			.padding(EdgeInsets(top: 7, leading: 10, bottom: 7, trailing: 10))
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
	}
}

private struct KeyHint: View {
	let keys: String
	let text: String

	var body: some View {
		HStack(spacing: 4) {
			Text(keys)
				.textStyle(Tokens.Typography.kbd)
				.padding(.horizontal, 5)
				.padding(.vertical, 3)
				.background(Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
			Text(text)
		}
	}
}

private struct HomeEyebrowText: View {
	let text: String

	var body: some View {
		Text(text.uppercased())
			.textStyle(Tokens.Typography.eyebrow)
			.tracking(0.44)
			.foregroundStyle(Tokens.Colors.inkMuted)
	}
}
