import AppKit
import SwiftUI
import VaultdayCore

/// Proportions of the Story board ("Story detail on Mac").
enum StoryLayout {
	static let panelWidth: CGFloat = 360
	static let railWidth: CGFloat = 44
	static let bodyMaxWidth: CGFloat = 760
	/// The Next Steps accordion's fixed height; the steps scroll inside it.
	static let accordionHeight: CGFloat = 248
}

/// The story screen: the header (number line, title, open-in buttons), the
/// note rendered the way Obsidian's Reading view shows it with the app's own
/// views in place of the task sections and the Activity query, and the Story
/// panel on the right. It takes the window below the header bar; Esc or
/// Planner goes back to the day.
struct StoryScreen: View {
	let app: AppModel
	let story: StoryModel

	var body: some View {
		if let content = story.content {
			HStack(spacing: 0) {
				VStack(alignment: .leading, spacing: 0) {
					StoryHeader(app: app, story: story, content: content)
					StoryBodyColumn(app: app, story: story, content: content)
				}
				.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
				.paneGround(Tokens.Colors.surface, isFocused: app.focus.focusedPane == .today)
				StoryPanelColumn(app: app, story: story, content: content)
					.frame(width: StoryLayout.panelWidth)
			}
		} else {
			VStack(spacing: Tokens.Spacing.space12) {
				Text(story.errorMessage ?? "The story could not be read")
					.font(Tokens.Typography.body.font)
					.foregroundStyle(Tokens.Colors.inkSecondary)
				Button("Back to the planner") { app.leaveStory() }
			}
			.frame(maxWidth: .infinity, maxHeight: .infinity)
			.background(Tokens.Colors.surface)
		}
	}
}

/// Only the number line, the title and the open-in buttons.
private struct StoryHeader: View {
	let app: AppModel
	let story: StoryModel
	let content: StoryContent

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space8) {
				Button {
					app.leaveStory()
				} label: {
					Label("Planner", systemImage: "chevron.left")
						.font(Tokens.Typography.callout.font.weight(.medium))
						.foregroundStyle(Tokens.Colors.inkSecondary)
				}
				.buttonStyle(.plain)
				.help("Back to the planner (Esc)")
				Spacer(minLength: Tokens.Spacing.space12)
				StoryOpenButton(title: "Obsidian", symbol: "arrow.up.forward.square", help: "Open the note in Obsidian at the section") {
					if let link = story.obsidianLink { NSWorkspace.shared.open(link) }
				}
				StoryOpenButton(title: "Claude Code", symbol: "terminal", help: "Open a Claude Code session for this story; nothing listens for the link yet") {
					app.handleLink(story.claudeCodeLink)
				}
			}
			Text(content.numberLine)
				.font(Tokens.Typography.mono.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.textSelection(.enabled)
			Text(content.title ?? content.number)
				.font(Tokens.Typography.titleLarge.font)
				.tracking(Tokens.Typography.titleLarge.tracking)
				.foregroundStyle(Tokens.Colors.ink)
				.fixedSize(horizontal: false, vertical: true)
		}
		.padding(EdgeInsets(top: 16, leading: Tokens.Spacing.space32, bottom: 14, trailing: Tokens.Spacing.space24))
		.overlay(alignment: .bottom) { Rectangle().fill(Tokens.Colors.lineFaint).frame(height: 1) }
	}
}

/// An open-in button of the header: its name, then the leave glyph.
struct StoryOpenButton: View {
	let title: String
	let symbol: String
	let help: String
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: 6) {
				Text(title)
				Image(systemName: symbol)
					.font(.system(size: 10))
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			.padding(.horizontal, 10)
			.frame(height: Tokens.Size.sizeControl)
			.foregroundStyle(Tokens.Colors.ink)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
			.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
		}
		.buttonStyle(.plain)
		.font(Tokens.Typography.body.font.weight(.medium))
		.help(help)
	}
}

/// The rendered note in a scroll view, under one "Edit in Obsidian" link
/// that opens the note at the section the pointer is over.
private struct StoryBodyColumn: View {
	let app: AppModel
	let story: StoryModel
	let content: StoryContent

	var body: some View {
		VStack(alignment: .leading, spacing: 0) {
			HStack(spacing: Tokens.Spacing.space8) {
				Text("Read-only · free text is edited in Obsidian")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
				Spacer()
				if let message = story.writeMessage {
					Label(message, systemImage: "exclamationmark.triangle")
						.font(Tokens.Typography.caption.font)
						.foregroundStyle(Tokens.Colors.caution)
				}
				Button {
					if let link = story.editLink(section: story.currentSection) { NSWorkspace.shared.open(link) }
				} label: {
					Label(story.currentSection.map { "Edit in Obsidian · \($0)" } ?? "Edit in Obsidian", systemImage: "square.and.pencil")
						.font(Tokens.Typography.callout.font.weight(.medium))
				}
				.buttonStyle(.link)
				.help("Opens the note in Obsidian at the section under the pointer")
			}
			.padding(.horizontal, Tokens.Spacing.space32)
			.padding(.vertical, Tokens.Spacing.space8)
			ScrollView {
				VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
					ForEach(sectioned, id: \.block.id) { item in
						StoryBlockRow(app: app, story: story, content: content, block: item.block)
							.onHover { isInside in
								if isInside { story.pointAt(section: item.section) }
							}
					}
					if !content.hasActivityInBody {
						StoryHeadingText(level: 2, spans: [StorySpan(text: "Activity")])
						StoryActivityList(content: content)
					}
				}
				.frame(maxWidth: StoryLayout.bodyMaxWidth, alignment: .leading)
				.padding(.horizontal, Tokens.Spacing.space32)
				.padding(.bottom, Tokens.Spacing.space32)
				.frame(maxWidth: .infinity, alignment: .leading)
			}
		}
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.today) }
	}

	/// Each top-level block with the heading it sits under.
	private var sectioned: [(block: StoryBlock, section: String?)] {
		var section: String?
		return content.blocks.map { block in
			if case let .heading(_, title, _, _) = block.kind { section = title }
			return (block, section)
		}
	}
}
