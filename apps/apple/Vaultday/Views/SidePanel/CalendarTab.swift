import SwiftUI
import VaultdayCore

/// The side panel's Calendar tab: the legend, the day grid of the day the
/// Today column shows, and where the calendars and intake lists are picked.
struct CalendarTab: View {
	let app: AppModel
	let panel: CalendarPanelModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			CalendarLegend()
			if let message = panel.errorMessage {
				Text(message)
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.danger)
					.fixedSize(horizontal: false, vertical: true)
			}
			if let day = panel.day {
				DayGrid(app: app, days: [day], showsHeadings: false)
			}
			CalendarSourcesPanel(app: app)
		}
	}
}

/// Event, Locked, Unlocked, Tracked: the four block styles, as the board's legend.
private struct CalendarLegend: View {
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		HStack(spacing: 10) {
			swatch("Event") { Rectangle().fill(accent.opacity(0.12)) }
			swatch("Locked") {
				Rectangle().fill(Tokens.Colors.surfaceRaised).overlay(Rectangle().strokeBorder(accent, lineWidth: 1))
			}
			swatch("Unlocked") {
				Rectangle().fill(accent.opacity(0.10))
					.overlay(Rectangle().strokeBorder(accent, style: StrokeStyle(lineWidth: 1, dash: [3, 2])))
					.opacity(0.7)
			}
			swatch("Tracked") { Rectangle().fill(accent.opacity(0.22)) }
		}
		.font(Tokens.Typography.caption.font)
		.foregroundStyle(Tokens.Colors.inkMuted)
	}

	private func swatch(_ name: String, @ViewBuilder shape: () -> some View) -> some View {
		HStack(spacing: Tokens.Spacing.space4) {
			shape().frame(width: 14, height: 10).accessibilityHidden(true)
			Text(name)
		}
	}
}

/// One or more day columns over shared hour rows: blocks in their lanes,
/// the past under a wash, the now line, and drops that pin a task.
struct DayGrid: View {
	let app: AppModel
	let days: [CalendarDay]
	let showsHeadings: Bool

	private static let labelWidth: CGFloat = 44
	private static let minimumHourHeight: CGFloat = 48

	var body: some View {
		let scale = CalendarGridScale.fitting(
			days: days,
			workingStart: app.dayPlan.workingHours.start.minuteOfDay,
			workingEnd: app.dayPlan.workingHours.end.minuteOfDay,
		)
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			if showsHeadings {
				HStack(spacing: 0) {
					Spacer().frame(width: Self.labelWidth)
					ForEach(days) { day in
						Text(day.heading)
							.font(Tokens.Typography.caption.font.weight(.semibold))
							.foregroundStyle(day.isToday ? Tokens.Colors.ink : Tokens.Colors.inkMuted)
							.frame(maxWidth: .infinity)
					}
				}
			}
			GeometryReader { geometry in
				let height = max(geometry.size.height, CGFloat(scale.hours.count) * Self.minimumHourHeight)
				ScrollView(.vertical) {
					ZStack(alignment: .topLeading) {
						HourRows(scale: scale, height: height)
						HStack(spacing: 0) {
							Spacer().frame(width: Self.labelWidth)
							ForEach(days) { day in
								DayColumn(app: app, day: day, scale: scale, height: height)
							}
						}
					}
					.frame(height: height)
				}
				.scrollIndicators(.automatic)
			}
			.overlay(alignment: .top) { Rectangle().fill(Tokens.Colors.line).frame(height: 1) }
		}
	}
}

/// The hour lines with their labels.
private struct HourRows: View {
	let scale: CalendarGridScale
	let height: CGFloat

	var body: some View {
		ZStack(alignment: .topLeading) {
			ForEach(scale.hours, id: \.self) { hour in
				let y = scale.y(forMinute: hour * 60, height: height)
				Text(CalendarGridScale.label(hour: hour))
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.padding(.top, Tokens.Spacing.space2)
					.offset(y: y)
				Rectangle()
					.fill(Tokens.Colors.line)
					.frame(height: 1)
					.offset(y: y)
			}
		}
		.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
	}
}

/// One date's blocks, wash, now line and drop target.
private struct DayColumn: View {
	let app: AppModel
	let day: CalendarDay
	let scale: CalendarGridScale
	let height: CGFloat
	@State private var isTargeted = false
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		GeometryReader { geometry in
			let width = geometry.size.width - 8
			ZStack(alignment: .topLeading) {
				Color.clear
				ForEach(day.blocks) { block in
					let top = scale.y(forMinute: block.startMinute, height: height)
					let bottom = scale.y(forMinute: block.endMinute, height: height)
					let laneWidth = width / CGFloat(block.lanes)
					BlockTile(app: app, block: block, date: day.date)
						.frame(width: max(laneWidth - 2, 12), height: max(bottom - top - 2, 16))
						.offset(x: CGFloat(block.lane) * laneWidth, y: top + 1)
				}
				pastWash
				nowLine
			}
			.background(isTargeted ? accent.opacity(0.06) : .clear)
			.contentShape(Rectangle())
			.dropDestination(for: String.self) { taskIds, location in
				guard let taskId = taskIds.first else { return false }
				return app.pin(taskId: taskId, startMinute: scale.minute(atY: location.y, height: height), on: day.date)
			} isTargeted: { isTargeted = $0 }
		}
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.lineFaint).frame(width: 1) }
	}

	/// Everything before now sits under a white wash at about half strength.
	@ViewBuilder private var pastWash: some View {
		if let end = pastEnd, end > 0 {
			Rectangle()
				.fill(Tokens.Colors.surfaceRaised.opacity(0.55))
				.frame(height: end)
				.allowsHitTesting(false)
				.accessibilityHidden(true)
		}
	}

	/// Where the past ends: the whole grid on an earlier day, the now line today.
	private var pastEnd: CGFloat? {
		if day.isPast { return height }
		guard let minute = day.nowMinute else { return nil }
		return CGFloat(scale.y(forMinute: minute, height: Double(height)))
	}

	/// The 2 px rule at the current time, its dot and its time.
	@ViewBuilder private var nowLine: some View {
		if let minute = day.nowMinute {
			let y = scale.y(forMinute: minute, height: height)
			ZStack(alignment: .leading) {
				Rectangle().fill(Tokens.Colors.sunrise).frame(height: 2)
				Circle().fill(Tokens.Colors.sunrise).frame(width: 8, height: 8).offset(x: -4)
				Text(DayTime(minuteOfDay: minute)?.text ?? "")
					.font(.system(size: 10, weight: .semibold))
					.foregroundStyle(Tokens.Colors.onSunrise)
					.padding(.horizontal, 3)
					.padding(.vertical, 1)
					.background(Tokens.Colors.sunrise, in: RoundedRectangle(cornerRadius: 3))
					.offset(x: 6)
			}
			.offset(y: y - 1)
			.allowsHitTesting(false)
			.accessibilityElement()
			.accessibilityLabel("Now, \(DayTime(minuteOfDay: minute)?.text ?? "")")
		}
	}
}

/// One block: the type icon, the full name, the second line of time and
/// state tokens, the tracked fill and, on task blocks, the padlock.
private struct BlockTile: View {
	let app: AppModel
	let block: CalendarBlock
	let date: String
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		GeometryReader { geometry in
			let isShort = geometry.size.height < 32
			ZStack(alignment: .topLeading) {
				ground(height: geometry.size.height)
				lines(isShort: isShort)
					.padding(.leading, block.isTask ? 8 : 11)
					.padding(.trailing, block.isTask ? 26 : 8)
					.padding(.vertical, isShort ? 1 : 4)
				if let taskId = block.taskId {
					Padlock(app: app, taskId: taskId, date: date, isLocked: block.kind == .locked)
						.padding(.top, 2)
						.padding(.trailing, 3)
						.frame(maxWidth: .infinity, alignment: .topTrailing)
				}
			}
		}
		.opacity(block.kind == .unlocked ? 0.7 : 1)
		.clipShape(RoundedRectangle(cornerRadius: block.isTask ? Tokens.Radius.radiusSm : Tokens.Radius.radiusXs))
		.modifier(TaskDrag(taskId: block.taskId))
		.accessibilityElement(children: .combine)
	}

	@ViewBuilder private func ground(height: CGFloat) -> some View {
		let shape = RoundedRectangle(cornerRadius: block.isTask ? Tokens.Radius.radiusSm : Tokens.Radius.radiusXs)
		switch block.kind {
		case .event:
			shape.fill(accent.opacity(0.12))
		case .locked:
			shape.fill(Tokens.Colors.surfaceRaised)
				.overlay(alignment: .top) {
					Rectangle().fill(accent.opacity(0.18)).frame(height: height * block.trackedShare)
				}
				.overlay(shape.strokeBorder(accent, lineWidth: 1))
		case .unlocked:
			shape.fill(accent.opacity(0.10))
				.overlay(alignment: .top) {
					Rectangle().fill(accent.opacity(0.18)).frame(height: height * block.trackedShare)
				}
				.overlay(shape.strokeBorder(accent, style: StrokeStyle(lineWidth: 1, dash: [3, 2])))
		}
	}

	@ViewBuilder private func lines(isShort: Bool) -> some View {
		if isShort {
			HStack(spacing: Tokens.Spacing.space4) {
				icon
				Text(block.title).font(titleFont).foregroundStyle(titleColor)
				Text("· \(block.detail)").font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
			}
			.lineLimit(1)
			.truncationMode(.tail)
		} else {
			VStack(alignment: .leading, spacing: 1) {
				HStack(spacing: 5) {
					icon
					Text(block.title).font(titleFont).foregroundStyle(titleColor)
				}
				Text(block.detail)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			.lineLimit(1)
			.truncationMode(.tail)
		}
	}

	private var icon: some View {
		Image(systemName: block.isTask ? "checkmark.circle" : "calendar")
			.font(.system(size: 10))
			.foregroundStyle(block.isTask ? Tokens.Colors.ink : Tokens.Colors.inkMuted)
			.accessibilityLabel(block.isTask ? "Task" : "Calendar event")
	}

	private var titleFont: Font {
		block.isTask ? .system(size: 12, weight: .semibold) : .system(size: 11, weight: .medium)
	}

	private var titleColor: Color {
		block.isTask ? Tokens.Colors.ink : Tokens.Colors.inkSecondary
	}
}

/// The pin control on a task block's right edge: closed is locked, open is unlocked.
private struct Padlock: View {
	let app: AppModel
	let taskId: String
	let date: String
	let isLocked: Bool
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		Button {
			app.togglePin(taskId: taskId, on: date)
		} label: {
			Image(systemName: isLocked ? "lock" : "lock.open")
				.font(.system(size: 10))
				.foregroundStyle(isLocked ? accent : Tokens.Colors.inkMuted)
				.contentTransition(.symbolEffect(.replace))
				.frame(width: 18, height: 18)
		}
		.buttonStyle(.plain)
		.accessibilityLabel(isLocked ? "Locked: time-blocked on the calendar. Click to unlock" : "Unlocked: floats with the day. Click to lock")
		.help(help)
	}

	private var help: String {
		let keys = app.keyLabel(.togglePin)
		return isLocked
			? "Locked · time-blocked on the Vaultday calendar\(keys.map { " · \($0) unlocks it" } ?? "")"
			: "Unlocked · floats as the day changes\(keys.map { " · \($0) or click locks it" } ?? "")"
	}
}

/// Task blocks can be dragged to another time, which pins them there.
private struct TaskDrag: ViewModifier {
	let taskId: String?

	func body(content: Content) -> some View {
		if let taskId {
			content.draggable(taskId)
		} else {
			content
		}
	}
}

/// Calendar and Reminders access, the calendars this workspace reads and
/// its intake lists; folded away once something is picked.
private struct CalendarSourcesPanel: View {
	let app: AppModel
	@State private var isExpanded = false

	var body: some View {
		DisclosureGroup(isExpanded: $isExpanded) {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				calendars
				Divider()
				reminders
			}
			.padding(.top, Tokens.Spacing.space6)
		} label: {
			Text("Calendars and Reminders lists")
				.font(Tokens.Typography.callout.font.weight(.semibold))
				.foregroundStyle(Tokens.Colors.inkSecondary)
		}
		.onAppear { isExpanded = !app.hasCalendarAccess || (app.workspace?.pickedCalendarIds.isEmpty ?? true) }
	}

	@ViewBuilder private var calendars: some View {
		if app.calendarStatus.isEmpty {
			Text("Vaultday plans around the calendars you pick and keeps time boxes in a private Vaultday calendar in iCloud")
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
			Button("Allow calendar access") {
				Task { await app.requestCalendarAccess() }
			}
		} else {
			Text(app.calendarStatus)
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.fixedSize(horizontal: false, vertical: true)
		}
		if !app.pickableCalendars.isEmpty, let workspace = app.workspace, workspace.phase == .open {
			HomeEyebrow(text: "Read for \(workspace.name)")
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

	@ViewBuilder private var reminders: some View {
		let intake = app.remindersIntake
		if !intake.hasAccess {
			Text("Items you add to an intake list in Reminders become tasks here, then leave Reminders")
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
			Button("Allow Reminders access") {
				Task { await app.requestRemindersAccess() }
			}
		}
		if let message = intake.message {
			Text(message)
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.fixedSize(horizontal: false, vertical: true)
		}
		if intake.hasAccess, !intake.lists.isEmpty, let workspace = app.workspace, workspace.phase == .open {
			HomeEyebrow(text: "Intake lists")
			ForEach(intake.lists) { list in
				IntakeListRow(app: app, workspace: workspace, list: list)
			}
			Text("Pick lists only you use: each item is deleted from Reminders once its task is written")
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.fixedSize(horizontal: false, vertical: true)
		}
	}
}

/// One Reminders list: whether it is an intake list, and the story it files into.
private struct IntakeListRow: View {
	let app: AppModel
	let workspace: WorkspaceModel
	let list: ReminderListInfo

	var body: some View {
		let picked = workspace.intakeLists.first { $0.listId == list.id }
		HStack(spacing: Tokens.Spacing.space8) {
			Toggle(isOn: Binding(
				get: { picked != nil },
				set: { app.setIntakeList(list, isPicked: $0, story: picked?.story) },
			)) {
				Text("\(list.title) · \(list.sourceTitle)")
					.font(Tokens.Typography.callout.font)
					.lineLimit(1)
			}
			.toggleStyle(.checkbox)
			Spacer(minLength: 0)
			if picked != nil {
				Picker("Files into", selection: Binding(
					get: { picked?.story ?? "" },
					set: { app.setIntakeList(list, isPicked: true, story: $0.isEmpty ? nil : $0) },
				)) {
					Text("Reminders note").tag("")
					ForEach(app.quickAdd.stories) { story in
						Text("\(story.number) \(story.title)").tag(story.number)
					}
				}
				.labelsHidden()
				.controlSize(.small)
				.frame(maxWidth: 150)
			}
		}
	}
}
