import Foundation

/// How the story screen reads what the core rendered: the flat node list
/// becomes a tree again (each node names its parent, which comes first).
enum StoryMapping {
	static func content(_ view: StoryScreenView, today: String) -> StoryContent {
		StoryContent(
			relativePath: view.relativePath,
			number: view.number,
			title: view.title,
			sourceKind: view.sourceKind,
			sourceHost: view.sourceHost,
			status: view.status,
			priority: view.priority,
			due: view.due,
			estimate: view.estimate,
			project: view.project,
			authorship: authorship(view.authorship),
			blocks: tree(view.nodes),
			nextSteps: view.nextSteps.enumerated().map { step($1, place: $0, section: "next") },
			pendingInput: view.pendingInput.enumerated().map { step($1, place: $0, section: "pending") },
			waiting: view.waiting.map {
				PreviewWaiting(taskId: $0.taskId, text: $0.text, assignedTo: $0.assignedTo, ageDays: $0.ageDays.map(Int.init))
			},
			activity: view.activity.map { PreviewMapping.activity($0, today: today) },
			obsidianLink: URL(string: view.obsidianLink),
		)
	}

	/// The nodes as a tree. A node whose parent is missing or comes after it
	/// is a top-level block, so a malformed list never drops text.
	static func tree(_ nodes: [RenderNodeView]) -> [StoryBlock] {
		var children: [Int: [Int]] = [:]
		var roots: [Int] = []
		for (index, node) in nodes.enumerated() {
			if let parent = node.parent.map(Int.init), parent < index {
				children[parent, default: []].append(index)
			} else {
				roots.append(index)
			}
		}
		func build(_ index: Int) -> StoryBlock {
			let node = nodes[index]
			return StoryBlock(
				id: index,
				line: Int(node.line),
				kind: kind(node.kind),
				children: (children[index] ?? []).map(build),
			)
		}
		return roots.map(build)
	}

	static func kind(_ kind: RenderNodeKind) -> StoryBlock.Kind {
		switch kind {
		case let .heading(level, title, spans, obsidianLink):
			.heading(level: Int(level), title: title, spans: spans.map(span), obsidianLink: URL(string: obsidianLink))
		case let .paragraph(spans):
			.paragraph(spans.map(span))
		case let .list(isOrdered):
			.list(isOrdered: isOrdered)
		case let .listItem(marker, spans, checkbox):
			.listItem(marker: marker, spans: spans.map(span), checkbox: checkbox.map { StoryCheckbox(state: PreviewMapping.state($0.state), taskId: $0.taskId) })
		case .quote:
			.quote
		case let .callout(kind, title, fold):
			.callout(kind: kind, title: title.map(span), fold: Self.fold(fold))
		case let .code(language, text):
			.code(language: language, text: text)
		case let .table(alignments, header, rows):
			.table(
				alignments: alignments.map(alignment),
				header: header.map { $0.spans.map(span) },
				rows: rows.map { $0.cells.map { $0.spans.map(span) } },
			)
		case let .embed(kind, target, section, resolvedPath, filePath, obsidianLink):
			.embed(StoryEmbed(
				kind: embedKind(kind),
				target: target,
				section: section,
				resolvedPath: resolvedPath,
				fileURL: filePath.map { URL(fileURLWithPath: $0) },
				obsidianLink: obsidianLink.flatMap(URL.init(string:)),
			))
		case let .query(language, source, view):
			.query(language: language, source: source, view: view.map(appView))
		case let .appView(view):
			.appView(appView(view))
		case .rule:
			.rule
		}
	}

	static func span(_ view: SpanView) -> StorySpan {
		StorySpan(
			text: view.text,
			isStrong: view.strong,
			isEmphasis: view.emphasis,
			isStrikethrough: view.strikethrough,
			isHighlight: view.highlight,
			isCode: view.code,
			link: view.link.flatMap(link),
		)
	}

	static func link(_ view: SpanLinkView) -> StorySpan.Link? {
		switch view {
		case let .url(url):
			URL(string: url).map(StorySpan.Link.url)
		case let .note(target, section, resolvedPath, obsidianLink):
			.note(target: target, section: section, resolvedPath: resolvedPath, obsidianLink: obsidianLink.flatMap(URL.init(string:)))
		}
	}

	static func appView(_ view: AppViewKind) -> StoryAppView {
		switch view {
		case .nextSteps: .nextSteps
		case .pendingInput: .pendingInput
		case .activity: .activity
		}
	}

	private static func fold(_ fold: CalloutFoldView) -> StoryCalloutFold {
		switch fold {
		case .fixed: .fixed
		case .expanded: .expanded
		case .collapsed: .collapsed
		}
	}

	private static func alignment(_ alignment: ColumnAlignmentView) -> StoryColumnAlignment {
		switch alignment {
		case .natural: .natural
		case .leading: .leading
		case .center: .center
		case .trailing: .trailing
		}
	}

	private static func embedKind(_ kind: EmbedKindView) -> StoryEmbedKind {
		switch kind {
		case .image: .image
		case .note: .note
		case let .plugin(name): .plugin(name)
		case .file: .file
		}
	}

	private static func authorship(_ view: AuthorshipView) -> StoryAuthorship {
		let kind: StoryAuthorship.Kind = switch view.kind {
		case .human: .human
		case .model: .model
		case .mixed: .mixed
		}
		return StoryAuthorship(kind: kind, modelUnits: Int(view.modelUnits), latestModelDate: view.latestModelDate)
	}

	private static func step(_ view: StoryStepView, place: Int, section: String) -> StoryStep {
		StoryStep(
			id: view.taskId ?? "\(section)#\(place)",
			taskId: view.taskId,
			text: view.text,
			state: PreviewMapping.state(view.state),
			depth: Int(view.depth),
			scheduled: view.scheduled,
			due: view.due,
			estimateMinutes: view.estimateMinutes.map(Int.init),
			completion: view.completion,
			assignedTo: view.assignedTo,
		)
	}
}

extension StoryHours {
	init(_ view: StoryHoursDetailView) {
		self.init(
			trackedMinutes: Int(view.trackedMinutes),
			loggedMinutes: Int(view.loggedMinutes),
			estimateMinutes: view.estimateMinutes.map(Int.init),
			remainingMinutes: Int(view.remainingMinutes),
			overtimeMinutes: Int(view.overtimeMinutes),
			isEstimateFromTasks: view.budget == .tasks,
		)
	}
}
