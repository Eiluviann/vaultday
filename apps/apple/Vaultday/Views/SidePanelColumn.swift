import SwiftUI
import VaultdayCore

/// The side panel tabs in rail order; only Calendar has content in this shell.
enum SidePanelTab: String, CaseIterable, Identifiable {
	case calendar = "Calendar"
	case hours = "Hours"
	case chasers = "Chasers"
	case backlog = "Backlog"
	case objectives = "Objectives"
	case dayLog = "Day log"

	var id: String { rawValue }

	var symbol: String {
		switch self {
		case .calendar: "calendar"
		case .hours: "clock"
		case .chasers: "arrow.uturn.backward.circle"
		case .backlog: "list.bullet"
		case .objectives: "target"
		case .dayLog: "text.book.closed"
		}
	}
}

/// The 400 px side panel: the open tab's body and the 44 px tab rail on its right edge.
struct SidePanelColumn: View {
	let app: AppModel
	@State private var tab: SidePanelTab = .calendar

	var body: some View {
		HStack(spacing: 0) {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
				Text(tab.rawValue)
					.font(Tokens.Typography.heading.font)
					.foregroundStyle(Tokens.Colors.ink)
				if tab == .calendar {
					CalendarAccessPanel(app: app)
				}
				Spacer()
			}
			.padding(EdgeInsets(top: 18, leading: 16, bottom: 18, trailing: 12))
			.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
			TabRail(selection: $tab)
		}
		.paneGround(Tokens.Colors.surfaceSunken, isFocused: app.focus.focusedPane == .sidePanel)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.sidePanel) }
	}
}

private struct TabRail: View {
	@Binding var selection: SidePanelTab

	var body: some View {
		VStack(spacing: Tokens.Spacing.space4) {
			ForEach(SidePanelTab.allCases) { tab in
				Button {
					selection = tab
				} label: {
					Image(systemName: tab.symbol)
						.frame(width: 32, height: 28)
						.background(selection == tab ? Tokens.Colors.surfaceRaised : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
				}
				.buttonStyle(.plain)
				.foregroundStyle(selection == tab ? Tokens.Colors.ink : Tokens.Colors.inkMuted)
				.help(tab.rawValue)
			}
			Spacer()
			Image(systemName: "command")
				.frame(width: 32, height: 28)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.help("Command palette (⌘K)")
		}
		.padding(.vertical, Tokens.Spacing.space12)
		.frame(width: HomeLayout.sidePanelRailWidth)
		.frame(maxHeight: .infinity)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
	}
}

/// Calendar access, the private calendar's state, and which calendars this workspace reads.
private struct CalendarAccessPanel: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			if app.calendarStatus.isEmpty {
				Text("Vaultday plans around the calendars you pick and keeps time boxes in a private Vaultday calendar in iCloud.")
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkSecondary)
				Button("Allow calendar access") {
					Task { await app.requestCalendarAccess() }
				}
			} else {
				Text(app.calendarStatus)
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkSecondary)
			}
			if !app.pickableCalendars.isEmpty, let workspace = app.workspace, workspace.phase == .open {
				Text("READ FOR \(workspace.name.uppercased())")
					.font(Tokens.Typography.eyebrow.font)
					.tracking(Tokens.Typography.eyebrow.tracking)
					.foregroundStyle(Tokens.Colors.inkMuted)
				ForEach(app.pickableCalendars) { calendar in
					Toggle(isOn: Binding(
						get: { workspace.pickedCalendarIds.contains(calendar.id) },
						set: { app.setCalendar(calendar, isPicked: $0) },
					)) {
						Text("\(calendar.title) · \(calendar.source.title)")
							.font(Tokens.Typography.callout.font)
					}
					.toggleStyle(.checkbox)
				}
			}
		}
	}
}
