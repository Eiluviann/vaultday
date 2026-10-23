import AppKit
import SwiftUI
import VaultdayCore

/// The task preview slide-over: the story header, the story note read-only,
/// the subtasks, the waiting items and the recent activity. It slides over the
/// Inbox and the side panel from the right edge; Esc or ✕ closes it.
struct TaskPreviewPanel: View {
	let app: AppModel
	let content: TaskPreviewContent
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		VStack(alignment: .leading, spacing: 12) {
			topBar
			Text(content.text)
				.font(Tokens.Typography.title.font)
				.foregroundStyle(Tokens.Colors.ink)
				.fixedSize(horizontal: false, vertical: true)
			facts
			if let notice = content.obsidianLinkNotice {
				Label(notice, systemImage: "info.circle")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.info)
					.lineLimit(1)
					.help(notice)
			}
			HStack(alignment: .top, spacing: 18) {
				storyNote
					.frame(maxWidth: .infinity, alignment: .topLeading)
				sidebar
					.frame(width: 250, alignment: .topLeading)
			}
			.frame(maxHeight: .infinity, alignment: .top)
		}
		.padding(.horizontal, 22)
		.padding(.vertical, 18)
		.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
		.background(Tokens.Colors.surfaceRaised)
		.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
		.shadow(color: .black.opacity(0.08), radius: 12, x: -6)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Task preview")
	}

	// MARK: Header

	private var topBar: some View {
		HStack(spacing: 10) {
			Button {
				app.preview.close()
			} label: {
				Image(systemName: "xmark")
					.font(.system(size: 10, weight: .semibold))
					.frame(width: 28, height: 24)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
					.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
			}
			.buttonStyle(.plain)
			.accessibilityLabel("Close preview")
			Text(content.headerLine)
				.font(.custom(Tokens.Typography.mono.family, size: 12))
				.foregroundStyle(Tokens.Colors.inkMuted)
				.lineLimit(1)
			if let status = content.storyStatusLabel {
				Text(status)
					.font(Tokens.Typography.callout.font.weight(.semibold))
					.foregroundStyle(Tokens.Colors.onWs)
					.padding(.horizontal, 10)
					.padding(.vertical, 2)
					.background(accent, in: Capsule())
			}
			Spacer(minLength: 8)
			if let link = content.obsidianLink {
				PreviewButton(title: "Obsidian", symbol: "arrow.up.forward.square", help: "Open the note in Obsidian") {
					NSWorkspace.shared.open(link)
				}
			}
			PreviewButton(title: "Claude Code", symbol: "terminal", help: "Open a Claude Code session for this task; nothing listens for the link yet") {
				app.handleLink(app.preview.claudeCodeLink(taskId: content.taskId))
			}
			if let number = content.storyNumber {
				Button {
					app.handleLink(app.preview.fullStoryLink(storyNumber: number))
				} label: {
					Text("Full story")
						.fontWeight(.semibold)
						.padding(.horizontal, 10)
						.frame(height: 28)
						.foregroundStyle(Tokens.Colors.onPrimary)
						.background(Tokens.Colors.primary, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
				}
				.buttonStyle(.plain)
				.font(Tokens.Typography.body.font)
				.help("Open the story screen: the note, its steps, the Story block and Hours")
			}
		}
	}

	private var facts: some View {
		FlowChips(chips: [
			content.estimateLabel.map { "Estimate \($0)" },
			content.scheduledLabel.map { "Scheduled \($0)" },
			content.dueLabel.map { "Due \($0)" },
			content.assignedTo.map { "Assigned to \($0)" },
			content.sectionTitle,
		].compactMap(\.self))
	}

	// MARK: Story note

	private var storyNote: some View {
		VStack(alignment: .leading, spacing: 10) {
			HomeEyebrow(text: "Story preview · read-only")
			ScrollView {
				VStack(alignment: .leading, spacing: 8) {
					if content.blocks.isEmpty {
						Text("The story note has no text beyond its tasks")
							.font(Tokens.Typography.callout.font)
							.foregroundStyle(Tokens.Colors.inkMuted)
					}
					ForEach(Array(content.blocks.enumerated()), id: \.offset) { _, block in
						NoteBlockRow(block: block)
					}
				}
				.frame(maxWidth: .infinity, alignment: .leading)
			}
		}
	}

	// MARK: Subtasks, waiting, activity

	private var sidebar: some View {
		ScrollView {
			VStack(alignment: .leading, spacing: 8) {
				HomeEyebrow(text: "Subtasks")
				if content.subtasks.isEmpty {
					quiet("No subtasks")
				}
				ForEach(content.subtasks) { subtask in
					HStack(spacing: Tokens.Spacing.space8) {
						Image(systemName: subtask.state == .done ? "checkmark.square.fill" : "square")
							.foregroundStyle(subtask.state == .done ? accent : Tokens.Colors.lineControl)
						Text(PreviewText.attributed(subtask.text))
							.strikethrough(subtask.state == .done)
							.fontWeight(subtask.state == .inProgress ? .semibold : .regular)
							.foregroundStyle(subtask.state == .done ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
							.frame(maxWidth: .infinity, alignment: .leading)
						if let estimate = subtask.estimateLabel {
							Text(estimate)
								.font(Tokens.Typography.caption.font)
								.foregroundStyle(Tokens.Colors.inkMuted)
						}
					}
					.font(Tokens.Typography.body.font)
				}
				HomeEyebrow(text: "Waiting on")
					.padding(.top, 10)
				if content.waiting.isEmpty {
					quiet("Nothing waits on anyone")
				}
				ForEach(content.waiting) { item in
					HStack {
						Text(item.text)
							.frame(maxWidth: .infinity, alignment: .leading)
						CautionChip(text: item.detail)
					}
					.font(Tokens.Typography.body.font)
				}
				HomeEyebrow(text: "Activity")
					.padding(.top, 10)
				if content.activity.isEmpty {
					quiet("No log lines about this story yet")
				}
				VStack(alignment: .leading, spacing: 4) {
					ForEach(content.activity) { entry in
						ActivityLine(entry: entry)
					}
				}
			}
			.frame(maxWidth: .infinity, alignment: .leading)
		}
	}

	private func quiet(_ text: String) -> some View {
		Text(text)
			.font(Tokens.Typography.callout.font)
			.foregroundStyle(Tokens.Colors.inkMuted)
	}
}

private struct PreviewButton: View {
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
			.frame(height: 24)
			.foregroundStyle(Tokens.Colors.ink)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
			.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
		}
		.buttonStyle(.plain)
		.font(Tokens.Typography.body.font.weight(.medium))
		.help(help)
	}
}

/// The fact pills under the title, wrapping onto more lines when they must.
private struct FlowChips: View {
	let chips: [String]

	var body: some View {
		FlowLayout(spacing: 8) {
			ForEach(chips, id: \.self) { chip in
				Text(chip)
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.ink)
					.padding(.horizontal, 10)
					.padding(.vertical, 3)
					.background(Tokens.Colors.fillControl, in: Capsule())
			}
		}
	}
}

private struct FlowLayout: Layout {
	var spacing: CGFloat

	func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
		arrange(width: proposal.width ?? .infinity, subviews: subviews).size
	}

	func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
		let placed = arrange(width: bounds.width, subviews: subviews)
		for (subview, origin) in zip(subviews, placed.origins) {
			subview.place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), proposal: .unspecified)
		}
	}

	private func arrange(width: CGFloat, subviews: Subviews) -> (size: CGSize, origins: [CGPoint]) {
		var origins: [CGPoint] = []
		var x: CGFloat = 0
		var y: CGFloat = 0
		var rowHeight: CGFloat = 0
		var widest: CGFloat = 0
		for subview in subviews {
			let size = subview.sizeThatFits(.unspecified)
			if x > 0, x + size.width > width {
				x = 0
				y += rowHeight + spacing
				rowHeight = 0
			}
			origins.append(CGPoint(x: x, y: y))
			x += size.width + spacing
			rowHeight = max(rowHeight, size.height)
			widest = max(widest, x - spacing)
		}
		return (CGSize(width: widest, height: y + rowHeight), origins)
	}
}

/// One block of the story note, drawn read-only.
private struct NoteBlockRow: View {
	let block: PreviewBlock

	var body: some View {
		switch block {
		case let .heading(level, text):
			Text(PreviewText.attributed(text))
				.font(level <= 2 ? Tokens.Typography.headline.font : Tokens.Typography.body.font.weight(.semibold))
				.foregroundStyle(Tokens.Colors.ink)
				.padding(.top, level <= 2 ? 6 : 2)
		case let .paragraph(text):
			Text(PreviewText.attributed(text))
				.font(Tokens.Typography.body.font)
				.lineSpacing(3)
				.foregroundStyle(Tokens.Colors.ink)
				.fixedSize(horizontal: false, vertical: true)
		case let .bullet(depth, text, state):
			HStack(alignment: .firstTextBaseline, spacing: 6) {
				switch state {
				case .none:
					Text("•").foregroundStyle(Tokens.Colors.inkMuted)
				case .some(.done):
					Image(systemName: "checkmark.square.fill").foregroundStyle(Tokens.Colors.inkMuted)
				case .some(.cancelled):
					Image(systemName: "minus.square").foregroundStyle(Tokens.Colors.inkMuted)
				case .some(.open), .some(.inProgress):
					Image(systemName: "square").foregroundStyle(Tokens.Colors.lineControl)
				}
				Text(PreviewText.attributed(text))
					.strikethrough(state == .done || state == .cancelled)
					.foregroundStyle(state == .done ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
					.fixedSize(horizontal: false, vertical: true)
			}
			.font(Tokens.Typography.body.font)
			.padding(.leading, CGFloat(depth) * 16)
		case let .code(text):
			Text(text)
				.font(.custom(Tokens.Typography.mono.family, size: 11))
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.textSelection(.enabled)
				.padding(.horizontal, 10)
				.padding(.vertical, 6)
				.frame(maxWidth: .infinity, alignment: .leading)
				.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
		}
	}
}
