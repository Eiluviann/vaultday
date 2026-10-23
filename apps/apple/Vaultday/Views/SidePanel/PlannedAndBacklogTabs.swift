import SwiftUI
import VaultdayCore

/// A rail tab that has no content in the base version: its name, a Planned
/// pill and one line on what will live here.
struct PlannedTab: View {
	let title: String
	let blurb: String

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			Text("Planned")
				.font(Tokens.Typography.callout.font.weight(.semibold))
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.padding(.horizontal, 9)
				.padding(.vertical, 4)
				.background(Tokens.Colors.fillControl, in: Capsule())
			Text(blurb)
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.fixedSize(horizontal: false, vertical: true)
		}
		.accessibilityElement(children: .combine)
		.accessibilityLabel("\(title), planned")
	}
}

/// The Backlog tab: the open Next Steps no day has claimed yet, which is the
/// Backlog preset of Browse in a narrow list.
struct BacklogTab: View {
	let app: AppModel
	let backlog: BrowseModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack {
				Text("\(backlog.taskCount) open, not scheduled")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
				Spacer()
				Button("Open in Browse") {
					if let preset = app.browse.presets.first(where: { $0.name == "Backlog" }) {
						app.showBrowse()
						app.browse.apply(preset)
						app.syncTodayItemCount()
					}
				}
				.buttonStyle(.plain)
				.font(Tokens.Typography.callout.font.weight(.semibold))
				.foregroundStyle(Tokens.Colors.link)
			}
			ScrollView {
				LazyVStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
					ForEach(backlog.groups) { group in
						HStack(spacing: Tokens.Spacing.space6) {
							if let number = group.storyNumber { MonoChip(text: number) }
							Text(group.title)
								.font(Tokens.Typography.callout.font.weight(.semibold))
								.foregroundStyle(Tokens.Colors.inkSecondary)
								.lineLimit(1)
						}
						.padding(.top, Tokens.Spacing.space6)
						ForEach(group.sections.flatMap(\.rows)) { row in
							HStack(spacing: Tokens.Spacing.space8) {
								Text(row.text)
									.font(Tokens.Typography.body.font)
									.foregroundStyle(Tokens.Colors.ink)
									.lineLimit(2)
									.frame(maxWidth: .infinity, alignment: .leading)
								if let estimate = row.estimateLabel {
									Text(estimate)
										.font(Tokens.Typography.caption.font)
										.foregroundStyle(Tokens.Colors.inkMuted)
								}
							}
							.padding(10)
							.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
							.contentShape(Rectangle())
							.onTapGesture { app.openPreview(taskId: row.id) }
						}
					}
					if backlog.groups.isEmpty {
						Text("Every open Next Step has a day")
							.font(Tokens.Typography.callout.font)
							.foregroundStyle(Tokens.Colors.inkMuted)
					}
				}
			}
		}
	}
}
