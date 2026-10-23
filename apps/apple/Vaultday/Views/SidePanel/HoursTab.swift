import SwiftUI
import VaultdayCore

/// The side panel's Hours tab in its v1 form: today's tracked time against
/// the daily target, then tracked, estimate and remaining per open story.
/// Tracked time is private and carries the hand glyph wherever it shows.
struct HoursTab: View {
	let hours: HoursModel

	var body: some View {
		ScrollView {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				if let message = hours.errorMessage {
					Text(message)
						.font(Tokens.Typography.callout.font)
						.foregroundStyle(Tokens.Colors.danger)
				}
				DailyTargetCard(hours: hours)
				if hours.rows.isEmpty {
					Text("No open story has an estimate or tracked time yet")
						.font(Tokens.Typography.callout.font)
						.foregroundStyle(Tokens.Colors.inkMuted)
						.padding(.top, Tokens.Spacing.space4)
				} else {
					HomeEyebrow(text: "Stories")
						.padding(.top, Tokens.Spacing.space8)
					ForEach(hours.rows) { row in
						StoryHoursCard(row: row)
					}
				}
			}
		}
		.scrollIndicators(.automatic)
	}
}

/// Tracked time is the user's alone: the hand says it never goes into a report.
struct PrivateTracked: View {
	let text: String

	var body: some View {
		Label {
			Text(text)
		} icon: {
			Image(systemName: "hand.raised")
				.font(.system(size: 10))
		}
		.labelStyle(.titleAndIcon)
		.help("Tracked by the timer; private to you, never in a report")
	}
}

private struct DailyTargetCard: View {
	let hours: HoursModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HStack {
				HomeEyebrow(text: "Today")
				Spacer()
				Text("target from hours per day")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			PrivateTracked(text: hours.targetText)
				.font(Tokens.Typography.headline.font)
				.foregroundStyle(Tokens.Colors.ink)
			ProgressView(value: hours.targetShare)
				.tint(Tokens.Colors.primary)
				.accessibilityLabel("Tracked today against the daily target")
		}
		.padding(Tokens.Spacing.space12)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
	}
}

private struct StoryHoursCard: View {
	let row: HoursRow

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			HStack(spacing: Tokens.Spacing.space6) {
				Text(row.title)
					.font(.system(size: 13, weight: .semibold))
					.foregroundStyle(Tokens.Colors.ink)
					.lineLimit(1)
				Spacer(minLength: 0)
				Text(row.number)
					.font(Tokens.Typography.mono.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			HStack(spacing: Tokens.Spacing.space8) {
				PrivateTracked(text: "Tracked \(DurationText.minutes(row.trackedMinutes))")
				Text(row.budgetText)
					.foregroundStyle(row.isOver ? Tokens.Colors.caution : Tokens.Colors.inkMuted)
				if row.isOver {
					Image(systemName: "exclamationmark.triangle")
						.font(.system(size: 10))
						.foregroundStyle(Tokens.Colors.caution)
						.accessibilityLabel("Over the estimate")
				}
			}
			.font(Tokens.Typography.caption.font)
			.foregroundStyle(Tokens.Colors.inkSecondary)
			.lineLimit(1)
			if let share = row.share {
				ProgressView(value: share)
					.tint(row.isOver ? Tokens.Colors.caution : Tokens.Colors.primary)
					.accessibilityLabel("Tracked against the estimate")
			}
		}
		.padding(Tokens.Spacing.space12)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
	}
}
