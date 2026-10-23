import SwiftUI
import VaultdayCore

/// The 220 px sidebar of the Home board: the workspace switcher, the Now
/// card, the rituals, Browse, the tracked ring and the health footer.
struct SidebarColumn: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			HomeEyebrow(text: "Workspace")
				.padding(.horizontal, Tokens.Spacing.space8)
				.padding(.vertical, Tokens.Spacing.space4)
			WorkspaceSwitcher(app: app)
			NowCard(app: app)
				.padding(.top, Tokens.Spacing.space8)
				.padding(.bottom, Tokens.Spacing.space2)
			HomeEyebrow(text: "Rituals")
				.padding(EdgeInsets(top: 14, leading: 8, bottom: 4, trailing: 8))
			RitualList(app: app)
			Rectangle()
				.fill(Tokens.Colors.line)
				.frame(height: 1)
				.padding(EdgeInsets(top: 10, leading: 4, bottom: 2, trailing: 4))
			BrowseRow(app: app, count: app.dayPlan.browseCount)
				.padding(.top, Tokens.Spacing.space4)
			Spacer(minLength: Tokens.Spacing.space8)
			SidebarNotices(app: app)
			TrackedRing(totals: app.dayPlan.totals)
				.padding(.top, Tokens.Spacing.space6)
			HealthFooter(app: app)
		}
		.font(Tokens.Typography.body.font)
		.padding(.vertical, Tokens.Spacing.space12)
		.padding(.horizontal, 10)
		.frame(width: HomeLayout.sidebarWidth, alignment: .leading)
		.frame(maxHeight: .infinity, alignment: .top)
		.paneGround(Tokens.Colors.surfaceSidebar, isFocused: app.focus.focusedPane == .sidebar)
		.overlay(alignment: .trailing) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.sidebar) }
	}
}

// MARK: Switcher

/// All workspaces (⌘0) and each workspace (⌘1 to ⌘9), with its color dot. Badges arrive with the signals.
private struct WorkspaceSwitcher: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			SwitcherRow(
				title: "All workspaces",
				dot: AnyShapeStyle(LinearGradient(stops: [.init(color: Tokens.Colors.wsBlue, location: 0.5), .init(color: Tokens.Colors.wsGreen, location: 0.5)], startPoint: .topLeading, endPoint: .bottomTrailing)),
				shortcut: app.keyLabel(.showAllWorkspaces),
				selectedFill: Tokens.Colors.primary,
				selectedText: Tokens.Colors.onPrimary,
				isSelected: app.isShowingAll,
			) {
				app.perform(.showAllWorkspaces)
			}
			ForEach(Array(app.workspaceEntries.enumerated()), id: \.element.id) { index, entry in
				SwitcherRow(
					title: entry.name,
					dot: AnyShapeStyle(entry.color),
					shortcut: index < 9 ? app.keyLabel(.showWorkspace(index + 1)) : nil,
					selectedFill: entry.color,
					selectedText: Tokens.Colors.onWs,
					isSelected: !app.isShowingAll && entry.id == app.workspace?.workspaceId,
				) {
					app.showWorkspace(entry)
				}
			}
			if app.workspaceEntries.isEmpty, let grantMessage = app.grantMessage {
				Text(grantMessage)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.caution)
			}
		}
	}
}

private struct SwitcherRow: View {
	let title: String
	let dot: AnyShapeStyle
	let shortcut: String?
	let selectedFill: Color
	let selectedText: Color
	let isSelected: Bool
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: Tokens.Spacing.space8) {
				Circle()
					.fill(isSelected ? AnyShapeStyle(Tokens.Colors.surfaceRaised) : dot)
					.frame(width: 10, height: 10)
				Text(title)
					.lineLimit(1)
				Spacer(minLength: Tokens.Spacing.space4)
				if let shortcut {
					Text(shortcut)
						.font(Tokens.Typography.caption.font.weight(isSelected ? .medium : .regular))
						.foregroundStyle(isSelected ? selectedText.opacity(0.85) : Tokens.Colors.inkMuted)
				}
			}
			.fontWeight(isSelected ? .semibold : .regular)
			.foregroundStyle(isSelected ? selectedText : Tokens.Colors.inkSecondary)
			.padding(.horizontal, Tokens.Spacing.space8)
			.padding(.vertical, 7)
			.background(isSelected ? selectedFill : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}
}

// MARK: Now card

/// The running timer, or the place to drop a task to start one.
private struct NowCard: View {
	let app: AppModel
	@State private var isTargeted = false
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		Group {
			if let session = app.sessions.openSession {
				running(session)
			} else {
				idle
			}
		}
		.dropDestination(for: String.self) { taskIds, _ in
			guard let taskId = taskIds.first else { return false }
			app.startTimer(taskId: taskId)
			return true
		} isTargeted: { isTargeted = $0 }
		.help("Drop a task here to start its timer")
	}

	private func running(_ session: SessionSummary) -> some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text(["NOW", session.taskId].compactMap(\.self).joined(separator: " · "))
				.font(.system(size: 10, weight: .bold))
				.tracking(0.6)
				.opacity(0.8)
			Text(app.taskText(session.taskId) ?? session.meetingTitle ?? session.story ?? "Timer")
				.font(Tokens.Typography.body.font.weight(.semibold))
				.lineLimit(2)
			HStack(spacing: Tokens.Spacing.space8) {
				TimelineView(.periodic(from: .now, by: 1)) { context in
					Text(DurationText.clock(seconds: session.elapsed(at: context.date)))
						.font(.system(size: 18, weight: .bold))
						.monospacedDigit()
				}
				Spacer()
				NowButton(symbol: session.isPaused ? "play.fill" : "pause.fill", label: session.isPaused ? "Resume timer" : "Pause timer", help: app.keyLabel(.toggleTimer).map { session.isPaused ? "Resume (\($0))" : "Pause (\($0))" } ?? "Pause") {
					app.sessions.togglePause()
				}
				NowButton(symbol: "stop.fill", label: "Stop timer", help: "Stop · ends the session") {
					app.sessions.stop()
				}
			}
			.padding(.top, Tokens.Spacing.space2)
		}
		.foregroundStyle(Tokens.Colors.onWs)
		.padding(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(accent, in: RoundedRectangle(cornerRadius: 10))
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Tokens.Colors.focus, lineWidth: isTargeted ? 2 : 0))
	}

	private var idle: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
			Text("NOW")
				.font(.system(size: 10, weight: .bold))
				.tracking(0.6)
				.foregroundStyle(Tokens.Colors.inkMuted)
			Text("Drop a task here to start its timer")
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
		}
		.padding(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(isTargeted ? accent.opacity(0.12) : .clear, in: RoundedRectangle(cornerRadius: 10))
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(isTargeted ? accent : Tokens.Colors.lineControl, style: StrokeStyle(lineWidth: 1, dash: [4, 3])))
	}
}

private struct NowButton: View {
	let symbol: String
	let label: String
	let help: String
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			Image(systemName: symbol)
				.font(.system(size: 9))
				.foregroundStyle(Tokens.Colors.onWs)
				.frame(width: 26, height: 26)
				.background(Tokens.Colors.onWs.opacity(0.22), in: RoundedRectangle(cornerRadius: 7))
		}
		.buttonStyle(.plain)
		.help(help)
		.accessibilityLabel(label)
	}
}

// MARK: Rituals

/// The rituals of the open workspace's sphere: inert rows with when each is next due.
private struct RitualList: View {
	let app: AppModel

	var body: some View {
		TimelineView(.everyMinute) { context in
			VStack(alignment: .leading, spacing: 0) {
				ForEach(app.ritualRows(now: context.date)) { row in
					HStack(spacing: Tokens.Spacing.space8) {
						Image(systemName: "circle")
							.font(.system(size: 10))
							.foregroundStyle(Tokens.Colors.lineControl)
						Text(row.title)
							.foregroundStyle(Tokens.Colors.ink)
							.lineLimit(1)
						Spacer(minLength: Tokens.Spacing.space4)
						HStack(spacing: 0) {
							Text(row.whenText)
							if row.isNext {
								Text(" · ")
								Text("next")
									.fontWeight(.semibold)
									.foregroundStyle(Tokens.Colors.caution)
							}
						}
						.font(Tokens.Typography.caption.font)
						.foregroundStyle(Tokens.Colors.inkMuted)
					}
					.padding(.horizontal, Tokens.Spacing.space8)
					.padding(.vertical, Tokens.Spacing.space6)
					.help("Rituals run in a later version; the time comes from Settings")
				}
			}
		}
	}
}

// MARK: Browse

/// Every open task line of the workspace; a click or `gb` opens the Browse table.
private struct BrowseRow: View {
	let app: AppModel
	let count: Int

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Image(systemName: "tablecells")
				.font(.system(size: 12))
				.foregroundStyle(Tokens.Colors.inkMuted)
			Text("Browse")
				.foregroundStyle(Tokens.Colors.ink)
			Spacer()
			Text("\(count) open")
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
			ActionCap(app: app, action: .showBrowse)
		}
		.padding(.horizontal, Tokens.Spacing.space8)
		.padding(.vertical, Tokens.Spacing.space6)
		.background(app.screen == .browse ? Tokens.Colors.fillHover : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
		.contentShape(Rectangle())
		.onTapGesture { app.perform(.showBrowse) }
		.accessibilityAddTraits(.isButton)
		.help("Every open task line of this workspace")
	}
}

// MARK: Foot

/// Development notices: an unsigned build, a keymap that did not load, the app state line.
private struct SidebarNotices: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			if let notice = app.unsignedNotice {
				Label(notice, systemImage: "exclamationmark.triangle")
					.foregroundStyle(Tokens.Colors.caution)
			}
			if let error = app.keymapError {
				Label(error, systemImage: "keyboard")
					.foregroundStyle(Tokens.Colors.danger)
			}
			if let error = app.sessions.errorMessage {
				Label(error, systemImage: "timer")
					.foregroundStyle(Tokens.Colors.danger)
			}
		}
		.font(Tokens.Typography.caption.font)
		.padding(.horizontal, Tokens.Spacing.space4)
	}
}

/// Tracked time against the day's plan.
private struct TrackedRing: View {
	let totals: DayTotals
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			ZStack {
				Circle().stroke(Tokens.Colors.line, lineWidth: 2.5)
				Circle()
					.trim(from: 0, to: fraction)
					.stroke(accent, style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
					.rotationEffect(.degrees(-90))
			}
			.frame(width: 18, height: 18)
			.padding(2)
			.accessibilityHidden(true)
			(Text(DurationText.minutes(totals.spentMinutes)).fontWeight(.bold).foregroundColor(Tokens.Colors.ink)
				+ Text(" tracked of \(DurationText.minutes(totals.plannedMinutes)) planned"))
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
		}
		.padding(.horizontal, 10)
		.padding(.vertical, Tokens.Spacing.space8)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 10))
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Tokens.Colors.line))
		.help("Tracked by the timer against the day's plan; \(DurationText.minutes(totals.loggedMinutes)) of it logged to stories")
		.accessibilityElement(children: .combine)
	}

	private var fraction: CGFloat {
		guard totals.plannedMinutes > 0 else { return 0 }
		return min(1, CGFloat(totals.spentMinutes) / CGFloat(totals.plannedMinutes))
	}
}

/// The vault's health dot and the gear that opens Settings.
private struct HealthFooter: View {
	let app: AppModel
	@State private var isShowingHealth = false

	var body: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			Button {
				isShowingHealth.toggle()
			} label: {
				HStack(spacing: Tokens.Spacing.space6) {
					Circle()
						.fill(dotColor)
						.frame(width: 7, height: 7)
					Text(health?.label ?? "Not open")
				}
			}
			.buttonStyle(.plain)
			.help(health?.summary ?? "No workspace is open")
			.popover(isPresented: $isShowingHealth, arrowEdge: .top) {
				HealthList(health: health, status: app.workspace?.appStateStatus ?? "")
			}
			Spacer()
			SettingsLink {
				Image(systemName: "gearshape")
					.font(.system(size: 12))
					.frame(width: 22, height: 22)
			}
			.buttonStyle(.plain)
			.help("Settings (⌘,)")
			.accessibilityLabel("Settings")
		}
		.font(Tokens.Typography.caption.font)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.padding(EdgeInsets(top: 4, leading: 8, bottom: 0, trailing: 8))
	}

	private var health: VaultHealth? { app.workspace?.health }

	private var dotColor: Color {
		guard let health else { return Tokens.Colors.inkDisabled }
		return health.isClean ? Tokens.Colors.positive : Tokens.Colors.caution
	}
}

private struct HealthList: View {
	let health: VaultHealth?
	let status: String

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HomeEyebrow(text: health.map { $0.isClean ? "Vault health" : "Vault health · \($0.issues.count) items" } ?? "Vault health")
			if let health {
				if health.isClean {
					Text("No conflicts, no lines without id")
				}
				ForEach(health.issues, id: \.self) { issue in
					Label(issue, systemImage: "exclamationmark.triangle")
						.foregroundStyle(Tokens.Colors.caution)
				}
				Divider()
				Text("\(health.files) files read")
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			if !status.isEmpty {
				Text(status)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.font(Tokens.Typography.callout.font)
		.padding(Tokens.Spacing.space12)
		.frame(width: 300, alignment: .leading)
	}
}

// MARK: Rail

/// The 44 px rail the sidebar collapses into: the workspace dots, the next
/// ritual's time, the timer ring, the health dot at the foot.
struct SidebarRail: View {
	let app: AppModel

	var body: some View {
		VStack(spacing: 10) {
			ForEach(app.workspaceEntries) { entry in
				let isCurrent = !app.isShowingAll && entry.id == app.workspace?.workspaceId
				Button {
					app.showWorkspace(entry)
				} label: {
					Circle()
						.fill(entry.color)
						.frame(width: 12, height: 12)
						.overlay(Circle().strokeBorder(Tokens.Colors.focus, lineWidth: isCurrent ? 2 : 0).padding(-3))
				}
				.buttonStyle(.plain)
				.help(entry.name)
				.accessibilityLabel(entry.name)
			}
			if let next = app.ritualRows(now: Date()).first(where: \.isNext) {
				Text(next.whenText.split(separator: " ").last.map(String.init) ?? next.whenText)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.help("\(next.title) · \(next.whenText)")
					.padding(.top, Tokens.Spacing.space8)
			}
			if app.sessions.openSession != nil {
				Circle()
					.stroke(app.accentColor, lineWidth: 2.5)
					.frame(width: 16, height: 16)
					.help("The timer is running")
			}
			Spacer()
			Circle()
				.fill(app.workspace?.health?.isClean == true ? Tokens.Colors.positive : Tokens.Colors.caution)
				.frame(width: 7, height: 7)
				.help(app.workspace?.health?.summary ?? "")
		}
		.padding(.vertical, Tokens.Spacing.space12)
		.frame(width: HomeLayout.railWidth)
		.frame(maxHeight: .infinity)
		.paneGround(Tokens.Colors.surfaceSidebar, isFocused: app.focus.focusedPane == .sidebar)
		.overlay(alignment: .trailing) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
	}
}
