import SwiftUI
import VaultdayCore

/// The story screen's right panel: the Story block pinned at the top, always
/// visible, then below a split line the tabs, laid out as the Home side
/// panel's with the rail starting at the split. `]` `[` and the digits switch
/// the tabs while the panel holds focus (Keymap.yaml, `pane: story-panel`).
struct StoryPanelColumn: View {
	let app: AppModel
	let story: StoryModel
	let content: StoryContent

	var body: some View {
		VStack(spacing: 0) {
			StoryFactsBlock(content: content)
				.padding(EdgeInsets(top: 18, leading: 16, bottom: 14, trailing: 16))
			Rectangle().fill(Tokens.Colors.line).frame(height: 1)
			HStack(spacing: 0) {
				VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
					Text(story.panelTab.rawValue)
						.font(Tokens.Typography.heading.font)
						.foregroundStyle(Tokens.Colors.ink)
					tabBody
					Spacer(minLength: 0)
				}
				.padding(EdgeInsets(top: 16, leading: 16, bottom: 18, trailing: 12))
				.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
				StoryPanelRail(app: app, story: story)
			}
		}
		.paneGround(Tokens.Colors.surfaceSunken, isFocused: app.focus.focusedPane == .storyPanel)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.contentShape(Rectangle())
		.onTapGesture { app.focusStoryPanel() }
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Story panel")
	}

	@ViewBuilder
	private var tabBody: some View {
		switch story.panelTab {
		case .hours:
			StoryHoursTab(hours: story.hours, togglWeek: story.togglWeek)
		case .reports:
			ReportsView(app: app, model: story.reports, showsClientPicker: false)
		case .claudeCode:
			PlannedTab(title: "Claude Code", blurb: "Claude Code sessions in this story's folder will show up here")
		}
	}
}

/// The Story block: label-and-value rows from the frontmatter and the
/// provenance summary.
private struct StoryFactsBlock: View {
	let content: StoryContent

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			Text("Story")
				.font(Tokens.Typography.heading.font)
				.foregroundStyle(Tokens.Colors.ink)
			Grid(alignment: .leading, horizontalSpacing: Tokens.Spacing.space12, verticalSpacing: Tokens.Spacing.space6) {
				ForEach(content.facts) { fact in
					GridRow {
						Text(fact.label)
							.foregroundStyle(Tokens.Colors.inkMuted)
						HStack(spacing: Tokens.Spacing.space4) {
							if fact.isModelText {
								Image(systemName: "sparkle")
									.font(.system(size: 10))
									.foregroundStyle(Tokens.Colors.ai)
									.accessibilityLabel("Model text in the note")
							}
							Text(fact.value)
								.fontWeight(fact.value == "Not set" ? .regular : .semibold)
								.foregroundStyle(fact.value == "Not set" ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
						}
						.help(fact.isModelText ? "Note authorship from the provenance markers" : "\(fact.label) as the story note holds it")
					}
				}
			}
			.font(Tokens.Typography.callout.font)
		}
		.frame(maxWidth: .infinity, alignment: .leading)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Story details")
	}
}

/// The story's Hours: tracked (private, the hand), logged against the
/// estimate, and what remains, kept apart; then the Toggl week's slot.
private struct StoryHoursTab: View {
	let hours: StoryHours?
	let togglWeek: TogglWeekModel
	@Environment(\.screenShare) private var screenShare

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			if let hours {
				VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
					row("Logged", hours.loggedText)
					row("Estimate", hours.estimateText)
					row("Remaining", hours.remainingText, isCaution: hours.isOver)
					if let share = hours.loggedShare {
						ProgressView(value: share)
							.tint(hours.isOver ? Tokens.Colors.caution : Tokens.Colors.primary)
							.progressDraw(value: share)
							.accessibilityLabel("Logged against the estimate")
					}
					Rectangle().fill(Tokens.Colors.lineFaint).frame(height: 1)
					HStack {
						PrivateTracked(text: "Tracked")
							.foregroundStyle(Tokens.Colors.inkMuted)
						Spacer()
						if screenShare?.hides(.storyHours) != true {
							Text(hours.trackedText)
								.fontWeight(.semibold)
								.foregroundStyle(Tokens.Colors.ink)
						}
					}
				}
				.font(Tokens.Typography.callout.font)
				.padding(Tokens.Spacing.space12)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
			} else {
				Text("Hours need the workspace's app state")
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			TogglWeekCard(week: togglWeek)
		}
	}

	private func row(_ label: String, _ value: String, isCaution: Bool = false) -> some View {
		HStack {
			Text(label).foregroundStyle(Tokens.Colors.inkMuted)
			Spacer()
			Text(value)
				.fontWeight(.semibold)
				.foregroundStyle(isCaution ? Tokens.Colors.caution : Tokens.Colors.ink)
		}
	}
}

/// The tab rail, starting at the split line.
private struct StoryPanelRail: View {
	let app: AppModel
	let story: StoryModel

	var body: some View {
		VStack(spacing: Tokens.Spacing.space4) {
			ForEach(StoryPanelTab.allCases) { tab in
				Button {
					story.showTab(tab)
					app.focusStoryPanel()
				} label: {
					Image(systemName: tab.symbol)
						.frame(width: 32, height: 28)
						.background(story.panelTab == tab ? Tokens.Colors.surfaceRaised : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
				}
				.buttonStyle(.plain)
				.foregroundStyle(story.panelTab == tab ? Tokens.Colors.ink : Tokens.Colors.inkMuted)
				.help(help(tab))
				.accessibilityLabel(tab.rawValue)
				.accessibilityAddTraits(story.panelTab == tab ? .isSelected : [])
			}
			Spacer()
		}
		.padding(.vertical, Tokens.Spacing.space12)
		.frame(width: StoryLayout.railWidth)
		.frame(maxHeight: .infinity)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
	}

	/// "Hours · ] and [ step the tabs, 1 to 3 pick one while the panel is focused".
	private func help(_ tab: StoryPanelTab) -> String {
		let next = app.keyLabel(.nextStoryPanelTab)
		let previous = app.keyLabel(.previousStoryPanelTab)
		let keys = [next, previous].compactMap(\.self).joined(separator: " and ")
		return keys.isEmpty ? tab.rawValue : "\(tab.rawValue) · \(keys) step the tabs while the panel is focused"
	}
}
