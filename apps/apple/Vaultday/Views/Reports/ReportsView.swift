import SwiftUI
import VaultdayCore

/// The Reports tab, in the story panel and the side panel: a range, the bundle
/// as a readable summary, Copy as JSON and as markdown, and Open in Claude
/// Code. No model runs here; the tab writes the bundle for Claude Code and
/// emits a link nothing handles yet.
struct ReportsView: View {
	let app: AppModel
	@Bindable var model: ReportsModel
	/// The side panel picks a client; the story panel reports on its own story.
	let showsClientPicker: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			controls
			actions
			if let message = model.errorMessage {
				Text(message)
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.danger)
			}
			if let written = model.writtenReport {
				WrittenReportRow(model: model, written: written)
			}
			if let notice = model.copyNotice {
				Text(notice)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			if let bundle = model.bundle {
				ScrollView {
					ReportSummary(bundle: bundle)
				}
				.scrollIndicators(.automatic)
			}
		}
		.onAppear { model.loadIfNeeded() }
	}

	private var controls: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			if showsClientPicker {
				Picker("Client", selection: Binding(get: { model.subject ?? "" }, set: { model.show(subject: $0, load: true) })) {
					ForEach(model.clients, id: \.self) { client in
						Text(client).tag(client)
					}
				}
				.pickerStyle(.menu)
				.labelsHidden()
				.accessibilityLabel("Client")
			}
			SegmentedChoice(
				label: "Range",
				options: ReportPreset.allCases.map { ($0, $0.rawValue) },
				selection: Binding(get: { model.preset }, set: { model.select(preset: $0) }),
				ground: .sunken,
				fills: true,
			)
			if model.preset == .custom {
				HStack(spacing: Tokens.Spacing.space8) {
					DatePicker("From", selection: Binding(get: { model.customFrom }, set: { model.setCustomRange(from: $0, to: model.customTo) }), displayedComponents: .date)
					DatePicker("To", selection: Binding(get: { model.customTo }, set: { model.setCustomRange(from: model.customFrom, to: $0) }), displayedComponents: .date)
				}
				.textStyle(Tokens.Typography.callout)
			}
			Text(model.rangeText)
				.textStyle(Tokens.Typography.mono)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
	}

	private var actions: some View {
		FlowRow(spacing: Tokens.Spacing.space6) {
			ReportActionButton(title: "Copy JSON", symbol: "curlybraces", help: help("Copy the report as JSON", .copyReportJSON), isEnabled: model.bundle != nil) {
				model.copyJSON()
			}
			ReportActionButton(title: "Copy markdown", symbol: "doc.on.doc", help: help("Copy the report as markdown", .copyReportMarkdown), isEnabled: model.bundle != nil) {
				model.copyMarkdown()
			}
			ReportActionButton(title: "Claude Code", symbol: "terminal", help: help("Write the report to _agent/reports for Claude Code; nothing opens the folder yet", .openReportInClaudeCode), isEnabled: model.bundle != nil) {
				app.openReportInClaudeCode()
			}
		}
	}

	private func help(_ text: String, _ action: KeyAction) -> String {
		app.keyLabel(action).map { "\(text) (\($0))" } ?? text
	}
}

private struct ReportActionButton: View {
	let title: String
	let symbol: String
	let help: String
	let isEnabled: Bool
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			SetupLabel(title: title, icon: symbol)
		}
		.buttonStyle(SetupButtonStyle(isSmall: true))
		.disabled(!isEnabled)
		.help(help)
	}
}

/// The path of the file Open in Claude Code wrote, shown and copyable.
private struct WrittenReportRow: View {
	let model: ReportsModel
	let written: WrittenReport

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text("Written for Claude Code")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
			HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space6) {
				Text(written.relativePath)
					.textStyle(Tokens.Typography.mono)
					.foregroundStyle(Tokens.Colors.ink)
					.textSelection(.enabled)
					.lineLimit(3)
					.truncationMode(.middle)
				Button {
					model.copyWrittenPath()
				} label: {
					Image(systemName: "doc.on.doc")
				}
				.buttonStyle(.plain)
				.help("Copy the full path")
				.accessibilityLabel("Copy the path")
			}
		}
		.padding(Tokens.Spacing.space8)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
	}
}

/// The bundle as a readable summary: counts, stories, hours and the log by kind.
private struct ReportSummary: View {
	let bundle: ReportBundle

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			counts
			HomeEyebrow(text: "Stories")
			ForEach(bundle.stories) { story in
				StoryBlock(story: story)
			}
			HomeEyebrow(text: "Hours")
			ReportHoursBlock(hours: bundle.hours)
			HomeEyebrow(text: "Log")
			if bundle.logGroups.isEmpty {
				Text("No log lines in this range")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			ForEach(bundle.logGroups) { group in
				LogGroupBlock(group: group)
			}
			HomeEyebrow(text: "Authorship")
			Text(ReportBundle.authorshipText)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
		.frame(maxWidth: .infinity, alignment: .leading)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Report")
	}

	private var counts: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			countRow("Stories", String(bundle.stories.count))
			countRow("Log lines", String(bundle.logLineCount))
			countRow("Sessions", sessionsText)
			countRow("Billing", bundle.billing?.rawValue ?? "Not set")
		}
		.textStyle(Tokens.Typography.callout)
	}

	private var sessionsText: String {
		bundle.unmeasuredSessionCount > 0
			? "\(bundle.sessionCount) · \(bundle.unmeasuredSessionCount) without a duration"
			: String(bundle.sessionCount)
	}

	private func countRow(_ label: String, _ value: String) -> some View {
		HStack {
			Text(label).foregroundStyle(Tokens.Colors.inkMuted)
			Spacer()
			Text(value).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
		}
	}
}

private struct StoryBlock: View {
	let story: ReportStory

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text(story.heading)
				.textStyle(Tokens.Typography.headline)
				.foregroundStyle(Tokens.Colors.ink)
			Text(story.status ?? "Status not set")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
			Text("Next Steps: \(story.nextSteps.text)")
			Text("Pending Input: \(story.pendingInput.text)")
		}
		.textStyle(Tokens.Typography.callout)
		.foregroundStyle(Tokens.Colors.inkSecondary)
		.padding(Tokens.Spacing.space8)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
	}
}

/// Hours by billing model. FTE shows logged hours and nothing else, and no
/// model shows tracked time or money: the core never hands those over.
private struct ReportHoursBlock: View {
	let hours: ReportHours

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			switch hours {
			case let .logged(minutes, byStory):
				total("Logged", minutes)
				stories(byStory)
			case let .fte(minutes, byStory):
				total("Logged", minutes)
				stories(byStory)
			case let .hourly(rounding, billableMinutes, billable, nonBillableMinutes, nonBillable):
				total("Billable", billableMinutes)
				stories(billable)
				Text("Sessions round up to \(rounding) min")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
				total("Non-billable", nonBillableMinutes)
				stories(nonBillable)
			case let .fixed(quoted, delivered, overrun, tasks):
				total("Quoted", quoted)
				total("Delivered", delivered)
				total("Unpaid overrun", overrun)
				ForEach(tasks) { task in
					VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
						Text(task.title).foregroundStyle(Tokens.Colors.ink)
						Text(task.summary)
							.textStyle(Tokens.Typography.caption)
							.foregroundStyle(task.overrunMinutes > 0 ? Tokens.Colors.caution : Tokens.Colors.inkMuted)
					}
				}
			}
		}
		.textStyle(Tokens.Typography.callout)
	}

	private func total(_ label: String, _ minutes: Int) -> some View {
		HStack {
			Text(label).foregroundStyle(Tokens.Colors.inkMuted)
			Spacer()
			Text(DurationText.minutes(minutes)).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
		}
	}

	private func stories(_ entries: [ReportStoryMinutes]) -> some View {
		ForEach(entries) { entry in
			HStack {
				Text(entry.title).textStyle(Tokens.Typography.mono).foregroundStyle(Tokens.Colors.inkSecondary)
				Spacer()
				Text(entry.text).foregroundStyle(Tokens.Colors.inkSecondary)
			}
		}
	}
}

private struct LogGroupBlock: View {
	let group: ReportLogGroup

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text("\(group.title) (\(group.lines.count))")
				.textStyle(Tokens.Typography.callout).fontWeight(.semibold)
				.foregroundStyle(Tokens.Colors.ink)
			ForEach(group.lines) { line in
				VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
					Text(line.text).foregroundStyle(Tokens.Colors.inkSecondary)
					Text(detail(line))
						.textStyle(Tokens.Typography.caption)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
			}
		}
		.textStyle(Tokens.Typography.callout)
	}

	private func detail(_ line: ReportLogLine) -> String {
		(["\(line.date) \(line.time)"] + line.references + (line.agent.map { ["written by \($0)"] } ?? [])).joined(separator: " · ")
	}
}
