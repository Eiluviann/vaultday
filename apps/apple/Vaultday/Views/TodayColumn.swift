import SwiftUI
import VaultdayCore

/// The Today column: the day's title and controls, its totals, quick add,
/// the task cards and the toast. Before any workspace is open it asks for a folder.
struct TodayColumn: View {
	let app: AppModel
	@State private var isPickingFolder = false

	var body: some View {
		Group {
			if let setupError = app.setupError {
				ContentUnavailableView("Vaultday could not start", systemImage: "exclamationmark.triangle", description: Text(setupError))
			} else if let workspace = app.workspace {
				switch workspace.phase {
				case .open:
					TodayPlan(app: app, plan: app.dayPlan)
				case .noFolder:
					grantPrompt(message: app.grantMessage ?? "Vaultday reads a workspace folder in place; nothing is copied or moved.")
				case let .failed(message):
					grantPrompt(message: message)
				}
			} else {
				ProgressView()
			}
		}
		.frame(maxWidth: .infinity, maxHeight: .infinity)
		.paneGround(Tokens.Colors.surface, isFocused: app.focus.focusedPane == .today)
		.fileImporter(isPresented: $isPickingFolder, allowedContentTypes: [.folder]) { result in
			if case let .success(url) = result {
				app.grantFolder(url)
			}
		}
	}

	private func grantPrompt(message: String) -> some View {
		ContentUnavailableView {
			Label("Open a workspace folder", systemImage: "folder")
		} description: {
			Text(message)
		} actions: {
			Button("Choose folder") { isPickingFolder = true }
				.buttonStyle(.borderedProminent)
		}
	}
}

/// The open workspace's day.
private struct TodayPlan: View {
	let app: AppModel
	let plan: DayPlanModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			DayHeader(
				title: plan.title,
				isToday: plan.isToday,
				app: app,
				showsScheduling: !plan.isPast,
				showsSpanSwitch: true,
			)
			DayTotalsRow(totals: plan.totals, remainingMinutes: plan.remainingMinutes, workloadMinutes: app.rhythm?.workloadThresholdMinutes)
			QuickAddField(app: app, quickAdd: app.quickAdd)
				.padding(.top, Tokens.Spacing.space2)
				.padding(.bottom, Tokens.Spacing.space6)
				.zIndex(2)
			if let message = plan.errorMessage {
				Text(message)
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.danger)
			}
			if app.calendarPanel.span != .list {
				DayGrid(app: app, days: app.calendarPanel.spanDays, showsHeadings: true)
			} else {
				TodayCards(app: app, plan: plan)
			}
			TipsLine(app: app)
		}
		.padding(18)
		.overlay(alignment: .bottom) {
			if let toast = plan.toast {
				PlanToastView(app: app, toast: toast)
					.padding(.bottom, 18)
					.transition(.move(edge: .bottom).combined(with: .opacity))
			}
		}
		.animation(.easeOut(duration: Tokens.Motion.motionTransition), value: plan.toast)
		.task(id: plan.toast) {
			// A toast that only reports dismisses itself; the over-free-time one stays until Esc or Re-plan.
			guard let toast = plan.toast, !toast.offersReplan else { return }
			try? await Task.sleep(for: .seconds(4))
			if plan.toast == toast { plan.dismissToast() }
		}
		.task { LaunchTimer.markFirstBody(todayMilliseconds: plan.lastCallMilliseconds) }
	}
}

/// The list of the day: the task cards, in order.
private struct TodayCards: View {
	let app: AppModel
	let plan: DayPlanModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HomeEyebrow(text: "Next steps")
				.padding(.top, Tokens.Spacing.space6)
			ScrollView {
				LazyVStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
					if plan.items.isEmpty {
						EmptyDay(title: plan.title, app: app)
					}
					ForEach(Array(plan.items.enumerated()), id: \.element.id) { index, item in
						TaskCard(
							app: app,
							item: item,
							isSelected: app.focus.focusedPane == .today && app.focus.selection(in: .today) == index,
						)
						.dropDestination(for: String.self) { payloads, _ in
							guard let payload = payloads.first, !item.isDone else { return false }
							return app.dropOnToday(payload, at: index)
						}
					}
				}
				.padding(.vertical, Tokens.Spacing.space4)
				.frame(maxWidth: .infinity, minHeight: 120, alignment: .top)
				.contentShape(Rectangle())
				// Below the cards: a task moves to the end, a signal becomes a task of the day.
				.dropDestination(for: String.self) { payloads, _ in
					guard let payload = payloads.first else { return false }
					return app.dropOnToday(payload, at: plan.items.count)
				}
			}
			.scrollIndicators(.automatic)
		}
	}
}

/// The day's title, the "today" return, ‹ ›, and the scheduling buttons.
struct DayHeader: View {
	let title: String
	let isToday: Bool
	let app: AppModel
	let showsScheduling: Bool
	/// The list, three-day and week switch; the Today column's alone.
	var showsSpanSwitch = false

	var body: some View {
		HStack(spacing: 10) {
			Text(title)
				.font(Tokens.Typography.titleLarge.font)
				.foregroundStyle(Tokens.Colors.ink)
				.lineLimit(1)
			Button {
				app.perform(.showToday)
			} label: {
				Text("today")
					.font(Tokens.Typography.caption.font.weight(.semibold))
					.foregroundStyle(isToday ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
					.padding(.horizontal, Tokens.Spacing.space8)
					.padding(.vertical, 3)
					.background(Tokens.Colors.fillControl, in: Capsule())
			}
			.buttonStyle(.plain)
			.keyboardHighlight(app.isHighlighted(DayHeaderControl.today), cornerRadius: 10)
			.disabled(isToday)
			.help(isToday ? "Today" : app.keyLabel(.showToday).map { "Back to today (\($0))" } ?? "Back to today")
			HStack(spacing: 0) {
				HeaderIconButton(symbol: "chevron.left", label: "Previous day", help: app.keyLabel(.previousDay).map { "Previous day (\($0))" } ?? "Previous day") {
					app.perform(.previousDay)
				}
				.keyboardHighlight(app.isHighlighted(DayHeaderControl.previousDay))
				HeaderIconButton(symbol: "chevron.right", label: "Next day", help: app.keyLabel(.nextDay).map { "Next day (\($0))" } ?? "Next day") {
					app.perform(.nextDay)
				}
				.keyboardHighlight(app.isHighlighted(DayHeaderControl.nextDay))
			}
			Spacer()
			if showsSpanSwitch {
				Picker("View", selection: Binding(get: { app.calendarPanel.span }, set: { app.showSpan($0) })) {
					ForEach(CalendarSpan.allCases) { span in
						Text(span.rawValue).tag(span)
					}
				}
				.pickerStyle(.segmented)
				.labelsHidden()
				.controlSize(.small)
				.fixedSize()
				.keyboardHighlight(app.isHighlighted(DayHeaderControl.span))
				.help("The day as a list, or the calendar over three days or a week")
			}
			if showsScheduling {
				HeaderIconButton(symbol: nil, text: "=", label: "Auto-schedule", help: hint("Auto-schedule: lays the unfinished, unlocked tasks into today's free time in list order", .autoSchedule)) {
					app.perform(.autoSchedule)
				}
				.keyboardHighlight(app.isHighlighted(DayHeaderControl.autoSchedule))
				HeaderIconButton(symbol: "wand.and.stars", label: "Re-plan from now", help: hint("Re-plan from now: moves only unfinished, unpinned tasks", .replan)) {
					app.perform(.replan)
				}
				.keyboardHighlight(app.isHighlighted(DayHeaderControl.replan))
			}
		}
		// The controls `h` and `l` walk once `k` climbs above the first card.
		.onAppear { app.setDayHeaderControls(controls) }
		.onChange(of: controls) { _, controls in app.setDayHeaderControls(controls) }
		// Browse and the story screen hold the Today pane without a header: `k` must find none there.
		.onDisappear { app.setDayHeaderControls([]) }
	}

	/// The header's controls left to right, as it shows them.
	private var controls: [DayHeaderControl] {
		var controls: [DayHeaderControl] = [.today, .previousDay, .nextDay]
		if showsSpanSwitch { controls.append(.span) }
		if showsScheduling { controls += [.autoSchedule, .replan] }
		return controls
	}

	private func hint(_ text: String, _ action: KeyAction) -> String {
		app.keyLabel(action).map { "\(text) (\($0))" } ?? text
	}
}

/// The day's numbers as split pills: estimate, spent and logged kept apart, then free time and done.
private struct DayTotalsRow: View {
	let totals: DayTotals
	let remainingMinutes: Int
	let workloadMinutes: Int?

	var body: some View {
		FlowRow(spacing: Tokens.Spacing.space6) {
			TotalPill(
				name: "Planned",
				value: DurationText.minutes(totals.plannedMinutes),
				isCaution: workloadMinutes.map { totals.plannedMinutes > $0 } ?? false,
				help: plannedHelp,
			)
			TotalPill(name: "Spent", value: DurationText.minutes(totals.spentMinutes), help: "Time the timer counted on this day", isPrivateTime: true)
			TotalPill(name: "Logged", value: DurationText.minutes(totals.loggedMinutes), help: "Timer time already allocated to a story")
			if let free = totals.freeMinutes {
				TotalPill(name: "Free", value: DurationText.minutes(free), isCaution: totals.overFreeTime(remainingMinutes: remainingMinutes) > 0, help: "Working time left around your calendars")
			}
			TotalPill(name: "Done", value: "\(totals.doneCount) / \(totals.taskCount)")
		}
	}

	private var plannedHelp: String {
		var parts = ["The estimates of the day's tasks"]
		if totals.unestimatedCount > 0 {
			parts.append(totals.unestimatedCount == 1 ? "1 task has no estimate" : "\(totals.unestimatedCount) tasks have no estimate")
		}
		if let workloadMinutes, totals.plannedMinutes > workloadMinutes {
			parts.append("over your workload of \(DurationText.minutes(workloadMinutes))")
		}
		return parts.joined(separator: "; ")
	}
}

/// The day without tasks.
private struct EmptyDay: View {
	let title: String
	let app: AppModel

	var body: some View {
		VStack(spacing: Tokens.Spacing.space6) {
			Image(systemName: "sun.horizon")
				.font(.system(size: 16))
				.foregroundStyle(Tokens.Colors.inkMuted)
			Text("No plan yet for \(title.split(separator: " ").first.map(String.init) ?? title)")
				.fontWeight(.semibold)
				.foregroundStyle(Tokens.Colors.inkSecondary)
			Text(app.keyLabel(.addTask).map { "Add the first task above, or press \($0)" } ?? "Add the first task above")
		}
		.font(Tokens.Typography.callout.font)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.multilineTextAlignment(.center)
		.padding(EdgeInsets(top: 18, leading: 16, bottom: 18, trailing: 16))
		.frame(maxWidth: .infinity)
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Tokens.Colors.lineControl, style: StrokeStyle(lineWidth: 1, dash: [4, 3])))
	}
}

/// The toast at the column's foot: over free time, with Re-plan.
private struct PlanToastView: View {
	let app: AppModel
	let toast: PlanToast

	var body: some View {
		HStack(spacing: 10) {
			if toast.offersReplan {
				Image(systemName: "exclamationmark.triangle")
					.foregroundStyle(Tokens.Colors.sunrise)
			}
			Text(toast.message)
			if toast.offersReplan {
				Button {
					app.perform(.replan)
				} label: {
					HStack(spacing: Tokens.Spacing.space8) {
						Text("Re-plan").fontWeight(.semibold)
						ActionCap(app: app, action: .replan, onFill: Tokens.Colors.onPrimary)
					}
					.padding(.horizontal, 9)
					.frame(height: 24)
					.background(Tokens.Colors.onPrimary.opacity(0.16), in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
				}
				.buttonStyle(.plain)
			}
			Button {
				app.dayPlan.dismissToast()
			} label: {
				Image(systemName: "xmark")
					.font(.system(size: 10, weight: .semibold))
					.frame(width: 22, height: 22)
			}
			.buttonStyle(.plain)
			.foregroundStyle(Tokens.Colors.lineControl)
			.accessibilityLabel("Dismiss")
		}
		.font(Tokens.Typography.callout.font)
		.foregroundStyle(Tokens.Colors.onPrimary)
		.padding(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 10))
		.background(Tokens.Colors.primary, in: RoundedRectangle(cornerRadius: 10))
		.shadow(color: .black.opacity(0.18), radius: 16, y: 10)
		.accessibilityElement(children: .contain)
		.accessibilityAddTraits(.updatesFrequently)
	}
}

/// The keys of the list, from the keymap, at the column's foot.
private struct TipsLine: View {
	let app: AppModel

	var body: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			Text("Drag to reorder or onto the calendar to lock")
			ForEach(app.dispatcher.tips, id: \.name) { tip in
				Text("· \(tip.name)")
				ActionCap(app: app, action: tip.action)
			}
			Spacer()
		}
		.font(Tokens.Typography.caption.font)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.padding(.top, 10)
	}
}
