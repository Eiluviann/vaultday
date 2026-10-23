import SwiftUI
import VaultdayCore

/// The side panel tabs in rail order: Calendar and Hours (PR 9a), Chasers,
/// Backlog, Objectives, Day log and Claude (PR 9b). Chasers, Objectives and
/// Claude are Planned in the base version.
enum SidePanelTab: String, CaseIterable, Identifiable {
	case calendar = "Calendar"
	case hours = "Hours"
	case chasers = "Chasers"
	case backlog = "Backlog"
	case objectives = "Objectives"
	case dayLog = "Day log"
	case claude = "Claude"

	var id: String { rawValue }

	/// Chasers, Objectives and Claude have no content in the base version.
	var isPlanned: Bool { [.chasers, .objectives, .claude].contains(self) }

	var symbol: String {
		switch self {
		case .calendar: "calendar"
		case .hours: "gauge.medium"
		case .chasers: "person.badge.clock"
		case .backlog: "tray.full"
		case .objectives: "target"
		case .dayLog: "doc.text"
		case .claude: "sparkles"
		}
	}
}

/// The 400 px side panel: the open tab's body and the 44 px tab rail on its right edge.
struct SidePanelColumn: View {
	@Bindable var app: AppModel

	var body: some View {
		HStack(spacing: 0) {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
				Text(app.sidePanelTab.rawValue)
					.font(Tokens.Typography.heading.font)
					.foregroundStyle(Tokens.Colors.ink)
				tabBody
				if app.sidePanelTab.isPlanned {
					Spacer()
				}
			}
			.padding(EdgeInsets(top: 18, leading: 16, bottom: 18, trailing: 12))
			.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
			TabRail(app: app, selection: $app.sidePanelTab)
		}
		.paneGround(Tokens.Colors.surfaceSunken, isFocused: app.focus.focusedPane == .sidePanel)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.sidePanel) }
	}
}

private struct TabRail: View {
	let app: AppModel
	@Binding var selection: SidePanelTab

	var body: some View {
		VStack(spacing: Tokens.Spacing.space4) {
			ForEach(SidePanelTab.allCases) { tab in
				Button {
					app.showSidePanelTab(tab)
				} label: {
					Image(systemName: tab.symbol)
						.symbolVariant(selection == tab ? .fill : .none)
						.iconReplace(value: selection == tab)
						.frame(width: 32, height: 28)
						.background(selection == tab ? Tokens.Colors.surfaceRaised : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
				}
				.buttonStyle(.plain)
				.foregroundStyle(selection == tab ? Tokens.Colors.ink : Tokens.Colors.inkMuted)
				.help(tab.rawValue)
				.accessibilityLabel(tab.rawValue)
				.accessibilityAddTraits(selection == tab ? .isSelected : [])
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

extension SidePanelColumn {
	/// The open tab's body.
	@ViewBuilder
	fileprivate var tabBody: some View {
		switch app.sidePanelTab {
		case .calendar:
			CalendarTab(app: app, panel: app.calendarPanel)
		case .hours:
			HoursTab(hours: app.hours)
		case .dayLog:
			DayLogTab(app: app, model: app.dayLog)
		case .backlog:
			BacklogTab(app: app, backlog: app.backlog)
		case .chasers:
			PlannedTab(title: "Chasers", blurb: "Open items that wait on someone resurface here after a few weekdays")
		case .objectives:
			PlannedTab(title: "Objectives", blurb: "Objectives and their key results will live here")
		case .claude:
			PlannedTab(title: "Claude", blurb: "Claude Code sessions for your stories will show up here")
		}
	}
}
