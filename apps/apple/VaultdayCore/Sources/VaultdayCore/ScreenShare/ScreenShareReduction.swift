import Foundation

/// What each private surface keeps while screen-share mode is on. All pure,
/// so the tests hold the rules.

extension SignalCard {
	/// The card as a shared screen sees it: the headline and the source icon.
	/// The sender, the address, the time, the subject, the excerpt and the
	/// ticket are gone; the id, the state and the source stay, for the
	/// outcomes the card still runs.
	public var reducedForScreenShare: SignalCard {
		SignalCard(
			id: id,
			source: source,
			sender: "",
			senderAddress: "",
			receivedAt: nil,
			timeLabel: "",
			headline: headline,
			subject: headline,
			excerpt: "",
			ticket: nil,
			ticketStoryTitle: nil,
			state: state,
		)
	}
}

extension PeekContent {
	/// The fields a peek keeps: what the row already shows.
	static let screenShareFieldLabels: Set<String> = [
		"Section", "Scheduled", "Due", "Estimate", "Subtasks", "Status", "Priority", "Time", "Kind", "Story", "Task", "From", "To",
	]

	/// The peek as the row shows it, nothing more: no excerpt, no activity
	/// lines, no waiting badge, and only the fields of ``screenShareFieldLabels``
	/// (not the assignee, the person waited on, the creation day or who wrote a line).
	public var reducedForScreenShare: PeekContent {
		PeekContent(
			chips: chips,
			subtitle: subtitle,
			badge: nil,
			title: title,
			fields: fields.filter { Self.screenShareFieldLabels.contains($0.label) },
			excerpt: nil,
			activity: [],
			taskId: taskId,
			storyNumber: storyNumber,
			obsidianLink: obsidianLink,
		)
	}
}

extension TaskPreviewContent {
	/// The preview with the story note, the people waited on, the activity
	/// lines and the assignee left out; the task, its facts and its subtasks stay.
	public var reducedForScreenShare: TaskPreviewContent {
		TaskPreviewContent(
			taskId: taskId,
			text: text,
			state: state,
			storyNumber: storyNumber,
			storyTitle: storyTitle,
			storyStatus: storyStatus,
			sectionTitle: sectionTitle,
			estimateLabel: estimateLabel,
			scheduledLabel: scheduledLabel,
			dueLabel: dueLabel,
			assignedTo: nil,
			blocks: [],
			subtasks: subtasks,
			waiting: [],
			activity: [],
			obsidianLink: obsidianLink,
			obsidianLinkNotice: obsidianLinkNotice,
		)
	}
}
