import SwiftUI
import VaultdayCore

/// The "Toggl · this week" slot of both Hours tabs: the week's days that
/// hold time for Toggl, each pushed, queued or failed, read from the queue
/// in the workspace document. "Toggl not connected" while the feature is off.
struct TogglWeekCard: View {
	let week: TogglWeekModel
	@Environment(\.screenShare) private var screenShare

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HomeEyebrow(text: TogglWeekModel.title)
			if let message = week.errorMessage {
				Text(message)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.danger)
			}
			ForEach(week.visibleDays) { day in
				HStack(spacing: Tokens.Spacing.space6) {
					Text(screenShare?.hides(.togglWeek) == true ? day.titleWithoutHours : day.title)
						.foregroundStyle(Tokens.Colors.ink)
					Spacer(minLength: Tokens.Spacing.space8)
					Text(day.stateText)
						.fontWeight(.semibold)
						.foregroundStyle(color(of: day.state))
				}
				.font(Tokens.Typography.callout.font)
				.accessibilityElement(children: .combine)
			}
			if let status = week.statusText {
				Text(status)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.fixedSize(horizontal: false, vertical: true)
			}
		}
		.padding(Tokens.Spacing.space12)
		.frame(maxWidth: .infinity, alignment: .leading)
		.overlay {
			RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg)
				.strokeBorder(Tokens.Colors.line, style: StrokeStyle(lineWidth: 1, dash: week.isConnected ? [] : [4, 3]))
		}
		.accessibilityElement(children: .contain)
		.accessibilityLabel(week.isConnected ? "Toggl this week" : "Toggl this week, not connected")
	}

	private func color(of state: TogglWeekDay.State) -> Color {
		switch state {
		case .pushed: Tokens.Colors.inkSecondary
		case .queued: Tokens.Colors.primary
		case .failed: Tokens.Colors.danger
		case .empty: Tokens.Colors.inkMuted
		}
	}
}
