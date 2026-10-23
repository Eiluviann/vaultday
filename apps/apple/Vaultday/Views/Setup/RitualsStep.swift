import SwiftUI
import VaultdayCore

/// Step 8: rituals and routines. Times and choices are stored in the app-level
/// document through ``RhythmModel``; rituals run in a later release, so the
/// times say Planned. The Settings window shows the same fields.
struct RitualsStep: View {
	let rhythm: RhythmModel

	var body: some View {
		StepPane(title: "Rituals and routines", lede: "Work gets a planned day with a start and an end. Life gets routines, reminders that ring on time, and the evenings and weekends") {
			SetupCallout(text: "Rituals open from a later release; the times you set are remembered for then") { PlannedTag() }
			if let message = rhythm.errorMessage {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: message)
			}
			RhythmSections(rhythm: rhythm, includesLife: true)
		}
	}
}

/// The day's settings in the order the board draws them: the choices for the
/// whole day, then Work's hours and rituals, then Life's. Shared by the step
/// and the Settings window.
struct RhythmSections: View {
	let rhythm: RhythmModel
	var includesLife: Bool

	var body: some View {
		LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: Tokens.Spacing.space12, alignment: .top)], spacing: Tokens.Spacing.space12) {
			dayCards
		}
		SpherePanel(sphere: .work) { EmptyView() } content: {
			workCards
		}
		if includesLife {
			SpherePanel(sphere: .life) { EmptyView() } content: {
				lifeCards
			}
		}
	}

	// MARK: Every day

	@ViewBuilder
	private var dayCards: some View {
		SetupCard(icon: "clock", title: "Breaks", description: "Focus and break length") {
			SetupMenu(label: "Breaks", options: BreakPatternSetting.allCases.map { ($0, $0.title) }, selection: Binding(get: { rhythm.breakPattern }, set: { rhythm.setBreakPattern($0) }), width: 110)
		}
		SetupCard(icon: "arrow.counterclockwise", title: "Roll-over", description: "Where unfinished tasks land") {
			SetupMenu(label: "Roll-over", options: ListPositionSetting.allCases.map { ($0, $0.rolloverTitle) }, selection: Binding(get: { rhythm.rolloverPosition }, set: { rhythm.setRolloverPosition($0) }), width: 130)
		}
		SetupCard(icon: "exclamationmark.triangle", title: "Overrun", description: "Warn when a day runs over") {
			SetupMenu(label: "Overrun warning", options: OverrunWarningSetting.allCases.map { ($0, $0.title) }, selection: Binding(get: { rhythm.overrunWarning }, set: { rhythm.setOverrunWarning($0) }), width: 130)
		}
	}

	// MARK: Work

	@ViewBuilder
	private var workCards: some View {
		SetupCard(icon: "clock", iconTone: .work, title: "Working hours", description: "Flexible drops set hours: Vaultday plans around your calendar and a daily target instead") {
			SegmentedChoice(label: "Working hours type", options: WorkingHoursKindSetting.allCases.map { ($0, $0.title) }, selection: Binding(get: { rhythm.workingHours.kind }, set: { rhythm.setWorkingHoursKind($0) }))
		} extra: {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				HStack(spacing: Tokens.Spacing.space6) {
					DayTimeField(label: "Work starts", time: rhythm.workingHours.start, issue: rhythm.issues["workingHours"], setTime: { rhythm.setWorkingHours(start: $0, end: rhythm.workingHours.end.text) })
					Text("–").foregroundStyle(Tokens.Colors.inkMuted)
					DayTimeField(label: "Work ends", time: rhythm.workingHours.end, issue: rhythm.issues["workingHours"], setTime: { rhythm.setWorkingHours(start: rhythm.workingHours.start.text, end: $0) })
				}
				HStack(spacing: Tokens.Spacing.space8) {
					ForEach(WeekdayChoice.allCases) { day in
						Toggle(day.shortTitle, isOn: Binding(get: { rhythm.workingHours.days.contains(day) }, set: { rhythm.setWorkDay(day, isOn: $0) }))
							.toggleStyle(.checkbox)
							.font(Tokens.Typography.callout.font)
					}
				}
				if let issue = rhythm.issues["workingHours"] {
					Text(issue).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.caution)
				}
			}
		}
		ritualCard(.planning, icon: "sunrise", description: "Pick today's tasks against the calendar")
		ritualCard(.shutdown, icon: "moon", description: "Roll over what is left, write the day log")
		ritualCard(.weeklyReview, icon: "checklist", description: "What moved, what slipped")
	}

	// MARK: Life

	@ViewBuilder
	private var lifeCards: some View {
		SetupCard(icon: "moon", iconTone: .life, title: "Plan Life after work", description: "Life tasks go into evenings and weekends") {
			SetupSwitch(label: "Plan Life after work", isOn: Binding(get: { rhythm.planAfterWorkForLife }, set: { rhythm.setPlanAfterWorkForLife($0) }))
		}
		SetupCard(icon: "repeat", iconTone: .life, title: "Routines", description: "Run, water the plants and the like arrive with routines", tags: [SetupTag(text: "Planned")]) {
			Button { } label: { SetupLabel(title: "Add") }
				.buttonStyle(SetupButtonStyle(isSmall: true))
				.disabled(true)
		}
		SetupCard(icon: "bell", iconTone: .life, title: "Reminder times ring", description: "Everything else waits for planning") {
			SetupSwitch(label: "Reminder times ring", isOn: Binding(get: { rhythm.remindersRing }, set: { rhythm.setRemindersRing($0) }))
		}
		ritualCard(.lightPlanning, icon: "sunrise", description: "A few minutes after work to pick tonight's tasks", tone: .life)
		ritualCard(.projectReview, icon: "folder", description: "Next steps for each personal project", tone: .life)
	}

	private func ritualCard(_ ritual: Ritual, icon: String, description: String, tone: SetupTag.Tone = .work) -> some View {
		SetupCard(icon: icon, iconTone: tone, title: ritual.title, description: description) {
			DayTimeField(
				label: "\(ritual.title) time",
				time: rhythm.time(of: ritual),
				day: rhythm.day(of: ritual),
				issue: rhythm.issues[ritual.rawValue],
				setTime: { rhythm.setTime($0, of: ritual) },
				setDay: { rhythm.setDay($0, of: ritual) },
			)
		}
	}
}
