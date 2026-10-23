import SwiftUI
import VaultdayCore

/// The side panel's Hours tab: today's tracked time against the daily
/// target, tracked, estimate and remaining per open story, then the Toggl
/// week. Tracked time is private and carries the hand glyph wherever it shows.
struct HoursTab: View {
	let hours: HoursModel
	@Environment(\.screenShare) private var screenShare

	var body: some View {
		ScrollView {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				if let message = hours.errorMessage {
					Text(message)
						.textStyle(Tokens.Typography.callout)
						.foregroundStyle(Tokens.Colors.danger)
				}
				// Today's tracked total is private; while sharing the card is not there at all.
				if screenShare?.hides(.hoursTab) != true {
					DailyTargetCard(hours: hours)
				}
				if hours.rows.isEmpty {
					EmptyState(icon: "hourglass", text: "No open story has an estimate or tracked time yet")
				} else {
					HomeEyebrow(text: "Stories")
						.padding(.top, Tokens.Spacing.space8)
					ForEach(hours.rows) { row in
						StoryHoursCard(row: row)
					}
				}
				TogglWeekCard(week: hours.togglWeek)
					.padding(.top, Tokens.Spacing.space8)
			}
		}
		.scrollIndicators(.automatic)
	}
}

/// Tracked time is the user's alone: the hand says it never goes into a
/// report, and screen-share mode leaves the value out with nothing in its place.
struct PrivateTracked: View {
	let text: String
	var label: String?

	var body: some View {
		PrivateTimeValue(text, label: label, carriesHand: true)
	}
}

private struct DailyTargetCard: View {
	let hours: HoursModel
	@Environment(\.screenShare) private var screenShare

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HStack {
				HomeEyebrow(text: "Today")
				Spacer()
				// The workspace's hours_per_day, which the design note calls the daily target.
				Text("Daily target")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			PrivateTracked(text: hours.targetText)
				.textStyle(Tokens.Typography.headline)
				.foregroundStyle(Tokens.Colors.ink)
			if screenShare?.hides(.hoursTab) != true {
				ProgressTrack(value: hours.targetShare)
					.accessibilityLabel("Tracked today against the daily target")
			}
		}
		.padding(Tokens.Spacing.space12)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
	}
}

private struct StoryHoursCard: View {
	let row: HoursRow
	@Environment(\.screenShare) private var screenShare

	private var isHidden: Bool { screenShare?.hides(.hoursTab) == true }

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			HStack(spacing: Tokens.Spacing.space6) {
				Text(row.title)
					.textStyle(Tokens.Typography.headline)
					.foregroundStyle(Tokens.Colors.ink)
					.lineLimit(1)
				Spacer(minLength: 0)
				Text(row.number)
					.textStyle(Tokens.Typography.mono)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			HStack(spacing: Tokens.Spacing.space8) {
				PrivateTracked(text: DurationText.minutes(row.trackedMinutes), label: "Tracked")
				Text(isHidden ? row.estimateOnlyText : row.budgetText)
					.foregroundStyle(row.isOver && !isHidden ? Tokens.Colors.caution : Tokens.Colors.inkMuted)
				if row.isOver, !isHidden {
					Image(systemName: "exclamationmark.triangle")
						.iconGlyph(.tag)
						.foregroundStyle(Tokens.Colors.caution)
						.accessibilityLabel("Over the estimate")
				}
			}
			.textStyle(Tokens.Typography.caption)
			.foregroundStyle(Tokens.Colors.inkSecondary)
			.lineLimit(1)
			if let share = row.share, !isHidden {
				ProgressTrack(value: share, tint: row.isOver ? Tokens.Colors.caution : Tokens.Colors.primary)
					.accessibilityLabel("Tracked against the estimate")
			}
		}
		.padding(Tokens.Spacing.space12)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
	}
}
