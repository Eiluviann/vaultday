import AppKit
import SwiftUI
import VaultdayCore

/// The spans of a block as one attributed string: bold, italics, code,
/// strikes and highlights as Obsidian draws them; a markdown link opens its
/// address and a wikilink opens its note in Obsidian.
enum StorySpanText {
	static func attributed(_ spans: [StorySpan]) -> AttributedString {
		var result = AttributedString()
		for span in spans {
			var part = AttributedString(span.text)
			var intent: InlinePresentationIntent = []
			if span.isStrong { intent.insert(.stronglyEmphasized) }
			if span.isEmphasis { intent.insert(.emphasized) }
			if span.isCode { intent.insert(.code) }
			if span.isStrikethrough { intent.insert(.strikethrough) }
			if !intent.isEmpty { part.inlinePresentationIntent = intent }
			if span.isHighlight { part.backgroundColor = Tokens.Colors.cautionSoft }
			switch span.link {
			case let .url(url):
				part.link = url
				part.foregroundColor = Tokens.Colors.info
			case let .note(_, _, _, obsidianLink?):
				part.link = obsidianLink
				part.foregroundColor = Tokens.Colors.info
			case .note:
				// A wikilink no note of the vault answers: shown, not followed.
				part.foregroundColor = Tokens.Colors.inkMuted
			case nil:
				break
			}
			result += part
		}
		return result
	}
}

/// A heading of the note.
struct StoryHeadingText: View {
	let level: Int
	let spans: [StorySpan]

	var body: some View {
		Text(StorySpanText.attributed(spans))
			.font(font)
			.foregroundStyle(Tokens.Colors.ink)
			.padding(.top, level <= 2 ? Tokens.Spacing.space12 : Tokens.Spacing.space4)
			.accessibilityAddTraits(.isHeader)
	}

	private var font: Font {
		switch level {
		case ...1: Tokens.Typography.title.font
		case 2: Tokens.Typography.headline.font.weight(.bold)
		default: Tokens.Typography.heading.font
		}
	}
}

/// One block of the rendered note and the blocks inside it.
struct StoryBlockRow: View {
	let app: AppModel
	let story: StoryModel
	let content: StoryContent
	let block: StoryBlock

	var body: some View {
		switch block.kind {
		case let .heading(level, _, spans, _):
			StoryHeadingText(level: level, spans: spans)
		case let .paragraph(spans):
			Text(StorySpanText.attributed(spans))
				.textStyle(Tokens.Typography.body)
				.lineSpacing(Tokens.Typography.body.lineSpacing + 2)
				.foregroundStyle(Tokens.Colors.ink)
				.textSelection(.enabled)
				.fixedSize(horizontal: false, vertical: true)
		case let .list(isOrdered):
			VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
				ForEach(Array(block.children.enumerated()), id: \.element.id) { place, item in
					StoryListItemRow(app: app, story: story, content: content, item: item, ordinal: isOrdered ? place + 1 : nil)
				}
			}
		case .listItem:
			StoryListItemRow(app: app, story: story, content: content, item: block, ordinal: nil)
		case .quote:
			children
				.padding(.leading, Tokens.Spacing.space12)
				.overlay(alignment: .leading) { Rectangle().fill(Tokens.Colors.lineButton).frame(width: 3) }
		case let .callout(kind, title, fold):
			StoryCalloutBox(kind: kind, title: title, fold: fold) { children }
		case let .code(language, text):
			VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
				if let language {
					Text(language)
						.textStyle(Tokens.Typography.caption)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
				Text(text)
					.textStyle(Tokens.Typography.mono)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.textSelection(.enabled)
					.fixedSize(horizontal: false, vertical: true)
			}
			.padding(.horizontal, Tokens.Spacing.space12)
			.padding(.vertical, Tokens.Spacing.space8)
			.frame(maxWidth: .infinity, alignment: .leading)
			.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
		case let .table(alignments, header, rows):
			StoryTable(alignments: alignments, header: header, rows: rows)
		case let .embed(embed):
			StoryEmbedView(embed: embed, fallbackLink: content.obsidianLink)
		case let .query(language, _, view):
			if view == .activity {
				StoryActivityList(content: content)
			} else {
				StoryPlaceholder(
					symbol: "line.3.horizontal.decrease.circle",
					title: "\(language.capitalizedQueryName) query, open in Obsidian",
					caption: "Vaultday never runs a query; Obsidian shows its results",
					link: story.editLink(section: section(of: block)),
				)
			}
		case .appView(.nextSteps):
			NextStepsSection(app: app, story: story)
		case .appView(.pendingInput):
			PendingInputList(app: app, story: story, content: content)
		case .appView(.activity):
			StoryActivityList(content: content)
		case .rule:
			Rectangle().fill(Tokens.Colors.line).frame(height: 1)
				.padding(.vertical, Tokens.Spacing.space4)
		}
	}

	private var children: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			ForEach(block.children) { child in
				StoryBlockRow(app: app, story: story, content: content, block: child)
			}
		}
	}

	/// The heading the block sits under, for the query's Obsidian link.
	private func section(of target: StoryBlock) -> String? {
		var section: String?
		for candidate in content.blocks {
			if case let .heading(_, title, _, _) = candidate.kind { section = title }
			if candidate.id == target.id { return section }
		}
		return section
	}
}

/// A list item: its bullet or checkbox, its text, and the items under it.
private struct StoryListItemRow: View {
	let app: AppModel
	let story: StoryModel
	let content: StoryContent
	let item: StoryBlock
	let ordinal: Int?

	var body: some View {
		if case let .listItem(_, spans, checkbox) = item.kind {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
				HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space6) {
					if let checkbox {
						StoryCheckboxButton(checkbox: checkbox) {
							if let taskId = checkbox.taskId {
								story.toggle(taskId: taskId, isDone: checkbox.isDone)
								app.storyChanged()
							}
						}
					} else if let ordinal {
						Text("\(ordinal).")
							.foregroundStyle(Tokens.Colors.inkMuted)
							.monospacedDigit()
					} else {
						Text("•").foregroundStyle(Tokens.Colors.inkMuted)
					}
					Text(StorySpanText.attributed(spans))
						.strikethrough(checkbox?.isDone ?? false)
						.foregroundStyle(checkbox?.isDone ?? false ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
						.textSelection(.enabled)
						.fixedSize(horizontal: false, vertical: true)
				}
				.textStyle(Tokens.Typography.body)
				ForEach(item.children) { child in
					StoryListItemRow(app: app, story: story, content: content, item: child, ordinal: nil)
						.padding(.leading, Tokens.Spacing.space20)
				}
			}
		}
	}
}

/// A task line's checkbox. Ticking writes through the core writer and
/// changes that one line; a line without an id is read-only.
struct StoryCheckboxButton: View {
	let checkbox: StoryCheckbox
	let action: () -> Void
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		// A read-only box is a plain image, not a disabled button: macOS shows
		// no tooltip on a disabled control, and the hover must say why.
		if checkbox.isTickable {
			Button(action: action) { glyph }
				.buttonStyle(.plain)
				.help(checkbox.isDone ? "Open the task again" : "Done, writes [x] in the note")
				.accessibilityLabel(checkbox.isDone ? "Done" : "Not done")
		} else {
			glyph
				.opacity(0.6)
				.help("This line has no id; tick it in Obsidian")
				.accessibilityLabel((checkbox.isDone ? "Done" : "Not done") + ", read-only: this line has no id")
		}
	}

	private var glyph: some View {
		Image(systemName: symbol)
			.foregroundStyle(checkbox.isDone ? accent : Tokens.Colors.lineControl)
			.iconGlyph(.row)
	}

	private var symbol: String {
		switch checkbox.state {
		case .done: "checkmark.square.fill"
		case .cancelled: "minus.square"
		case .inProgress: "square.lefthalf.filled"
		case .open: "square"
		}
	}
}

/// An Obsidian callout: its kind's glyph and color, its title, and its blocks;
/// a foldable one opens and folds.
private struct StoryCalloutBox<Inner: View>: View {
	let kind: String
	let title: [StorySpan]
	let fold: StoryCalloutFold
	@ViewBuilder let inner: () -> Inner
	@State private var isFolded: Bool

	init(kind: String, title: [StorySpan], fold: StoryCalloutFold, @ViewBuilder inner: @escaping () -> Inner) {
		self.kind = kind
		self.title = title
		self.fold = fold
		self.inner = inner
		_isFolded = State(initialValue: fold == .collapsed)
	}

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space6) {
				Image(systemName: style.symbol)
					.foregroundStyle(style.color)
				Text(title.isEmpty ? AttributedString(kind.capitalized) : StorySpanText.attributed(title))
					.textStyle(Tokens.Typography.body).fontWeight(.semibold)
					.foregroundStyle(Tokens.Colors.ink)
				Spacer(minLength: 0)
				if fold != .fixed {
					Button {
						isFolded.toggle()
					} label: {
						Image(systemName: isFolded ? "chevron.right" : "chevron.down")
							.foregroundStyle(Tokens.Colors.inkMuted)
					}
					.buttonStyle(.plain)
					.accessibilityLabel(isFolded ? "Open the callout" : "Fold the callout")
				}
			}
			if !isFolded {
				inner()
			}
		}
		.padding(Tokens.Spacing.space12)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(style.ground, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
	}

	private var style: (symbol: String, color: Color, ground: Color) {
		switch kind {
		case "warning", "caution", "attention":
			("exclamationmark.triangle", Tokens.Colors.caution, Tokens.Colors.cautionSoft)
		case "danger", "error", "bug", "failure", "fail", "missing":
			("xmark.octagon", Tokens.Colors.danger, Tokens.Colors.dangerSoft)
		case "success", "check", "done":
			("checkmark.circle", Tokens.Colors.positive, Tokens.Colors.positiveSoft)
		case "tip", "hint", "important":
			("lightbulb", Tokens.Colors.info, Tokens.Colors.infoSoft)
		case "question", "help", "faq":
			("questionmark.circle", Tokens.Colors.info, Tokens.Colors.infoSoft)
		case "quote", "cite":
			("quote.opening", Tokens.Colors.inkMuted, Tokens.Colors.surfaceSunken)
		default:
			("info.circle", Tokens.Colors.info, Tokens.Colors.infoSoft)
		}
	}
}

/// A pipe table, its columns aligned as the delimiter row says.
private struct StoryTable: View {
	let alignments: [StoryColumnAlignment]
	let header: [[StorySpan]]
	let rows: [[[StorySpan]]]

	var body: some View {
		Grid(alignment: .leading, horizontalSpacing: Tokens.Spacing.space16, verticalSpacing: Tokens.Spacing.space6) {
			GridRow {
				ForEach(Array(header.enumerated()), id: \.offset) { column, cell in
					Text(StorySpanText.attributed(cell))
						.textStyle(Tokens.Typography.body).fontWeight(.semibold)
						.gridColumnAlignment(alignment(column))
				}
			}
			Divider()
			ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
				GridRow {
					ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
						Text(StorySpanText.attributed(cell))
							.textStyle(Tokens.Typography.body)
					}
				}
			}
		}
		.foregroundStyle(Tokens.Colors.ink)
		.padding(Tokens.Spacing.space12)
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd).strokeBorder(Tokens.Colors.line))
	}

	private func alignment(_ column: Int) -> HorizontalAlignment {
		switch alignments.indices.contains(column) ? alignments[column] : .natural {
		case .center: .center
		case .trailing: .trailing
		case .natural, .leading: .leading
		}
	}
}

/// An embed: an image of the vault drawn in place; a note, a plugin's
/// drawing or another file as a placeholder with its Obsidian link.
private struct StoryEmbedView: View {
	let embed: StoryEmbed
	let fallbackLink: URL?

	var body: some View {
		switch embed.kind {
		case .image:
			if let url = embed.fileURL, let image = NSImage(contentsOf: url) {
				Image(nsImage: image)
					.resizable()
					.scaledToFit()
					.frame(maxWidth: min(image.size.width, 560), alignment: .leading)
					.clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
					.accessibilityLabel(embed.target)
			} else {
				StoryPlaceholder(symbol: "photo", title: embed.label, caption: "Image not found in the vault", link: nil)
			}
		case let .plugin(name):
			StoryPlaceholder(symbol: "scribble.variable", title: "\(name) · \(embed.label)", caption: "Drawn by an Obsidian plugin, open in Obsidian", link: embed.obsidianLink ?? fallbackLink)
		case .note:
			StoryPlaceholder(symbol: "doc.text", title: embed.label, caption: embed.resolvedPath == nil ? "Embedded note not found in the vault" : "Embedded note, open in Obsidian", link: embed.obsidianLink)
		case .file:
			StoryPlaceholder(symbol: "doc", title: embed.label, caption: "Embedded file, open in Obsidian", link: embed.obsidianLink ?? fallbackLink)
		}
	}
}

/// What the app does not draw itself: a labelled box with a link to Obsidian.
struct StoryPlaceholder: View {
	let symbol: String
	let title: String
	let caption: String
	let link: URL?

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Image(systemName: symbol)
				.iconGlyph(.menu)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.frame(width: 20)
			VStack(alignment: .leading, spacing: 2) {
				Text(title)
					.textStyle(Tokens.Typography.body).fontWeight(.semibold)
					.foregroundStyle(Tokens.Colors.ink)
				Text(caption)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			Spacer(minLength: Tokens.Spacing.space8)
			if let link {
				Button("Open in Obsidian") { NSWorkspace.shared.open(link) }
					.buttonStyle(.link)
					.textStyle(Tokens.Typography.callout)
			}
		}
		.padding(Tokens.Spacing.space12)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
	}
}

/// The story's activity: the day-log lines about it and its tasks, newest first.
struct StoryActivityList: View {
	let content: StoryContent

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			if content.activity.isEmpty {
				Text("No log lines about this story yet")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			ForEach(content.activity) { entry in
				ActivityLine(entry: entry)
			}
		}
		.padding(Tokens.Spacing.space12)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
	}
}

extension String {
	/// "dataview" → "Dataview", "dataviewjs" → "DataviewJS", "query" → "Search".
	fileprivate var capitalizedQueryName: String {
		switch self {
		case "dataviewjs": "DataviewJS"
		case "query": "Search"
		default: prefix(1).uppercased() + dropFirst()
		}
	}
}
