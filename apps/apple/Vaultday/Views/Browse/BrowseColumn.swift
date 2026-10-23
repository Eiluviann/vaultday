import SwiftUI
import VaultdayCore

/// The column widths of the Browse table, shared by its header and its rows.
private enum BrowseColumns {
	static let waiting: CGFloat = 96
	static let scheduled: CGFloat = 84
	static let due: CGFloat = 72
	static let estimate: CGFloat = 64
	static let story: CGFloat = 104
	static let id: CGFloat = 76
}

/// Browse: every task of the workspace as a table grouped by story, then
/// section, with a `/` query line and saved presets. It takes the place of
/// Today, Inbox and the side panel while shown.
struct BrowseColumn: View {
	let app: AppModel
	let browse: BrowseModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			header
			PresetRow(app: app, browse: browse)
			QueryLine(app: app, browse: browse)
			if let message = browse.errorMessage {
				Text(message)
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.danger)
			}
			TableHeader()
			ScrollViewReader { proxy in
				ScrollView {
					LazyVStack(alignment: .leading, spacing: 0) {
						if browse.groups.isEmpty {
							emptyState
						}
						ForEach(Array(browse.items.enumerated()), id: \.element.id) { index, item in
							line(item, index: index)
								.id(item.id)
						}
					}
					.padding(.bottom, Tokens.Spacing.space12)
				}
				.onChange(of: app.focus.selection(in: .today)) { _, index in
					if let index, browse.items.indices.contains(index) {
						proxy.scrollTo(browse.items[index].id)
					}
				}
			}
			footer
		}
		.padding(18)
		.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
		.paneGround(Tokens.Colors.surface, isFocused: app.focus.focusedPane == .today)
		.onAppear { app.syncTodayItemCount() }
		.onChange(of: browse.items) { app.syncTodayItemCount() }
		.contentShape(Rectangle())
		.onTapGesture { app.focus.focus(.today) }
	}

	private var header: some View {
		HStack(alignment: .firstTextBaseline, spacing: 12) {
			Text("Browse")
				.textStyle(Tokens.Typography.titleLarge)
				.foregroundStyle(Tokens.Colors.ink)
			Text(browse.summary)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkMuted)
			Spacer()
			Button {
				app.leaveBrowse()
			} label: {
				Label("Today", systemImage: "chevron.left")
					.labelStyle(TightLabelStyle())
					.textStyle(Tokens.Typography.callout).fontWeight(.semibold)
			}
			.buttonStyle(.plain)
			.foregroundStyle(Tokens.Colors.inkSecondary)
			.help("Back to the day")
		}
	}

	private var emptyState: some View {
		EmptyState(
			icon: browse.appliedQuery.isEmpty ? "tablecells" : "magnifyingglass",
			text: browse.appliedQuery.isEmpty ? "No open tasks" : "Nothing matches",
			detail: browse.appliedQuery.isEmpty ? "Tasks under Next Steps and Pending Input of a story show here" : "Change the query or pick another preset",
		)
		.padding(.top, Tokens.Spacing.space40)
	}

	private var footer: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			ForEach(tips, id: \.keys) { tip in
				HStack(spacing: Tokens.Spacing.space4) {
					Text(tip.keys)
						.textStyle(Tokens.Typography.kbd)
						.padding(.horizontal, 4)
						.padding(.vertical, 2)
						.background(Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
					Text(tip.what)
				}
			}
			Spacer()
		}
		.textStyle(Tokens.Typography.caption)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.lineLimit(1)
	}

	/// The keys the footer names, as the keymap binds them under the current scheme.
	private var tips: [(keys: String, what: String)] {
		[
			(app.keyLabel(.toggleFold), "fold"),
			([app.keyLabel(.nextGroup), app.keyLabel(.previousGroup)].compactMap(\.self).joined(separator: " "), "jump between groups"),
			(app.keyLabel(.peek), "peek"),
			(app.keyLabel(.open), "preview"),
			(app.keyLabel(.focusBrowseQuery), "query"),
		].compactMap { keys, what in
			guard let keys, !keys.isEmpty else { return nil }
			return (keys, what)
		}
	}

	@ViewBuilder
	private func line(_ item: BrowseItem, index: Int) -> some View {
		let isSelected = app.focus.focusedPane == .today && app.focus.selection(in: .today) == index
		switch item {
		case let .header(group):
			GroupHeader(app: app, browse: browse, group: group, isSelected: isSelected, index: index)
		case let .row(row):
			VStack(alignment: .leading, spacing: 0) {
				if let title = sectionTitle(before: row) {
					SectionLabel(text: title)
				}
				TaskRow(app: app, row: row, isSelected: isSelected, index: index)
			}
		}
	}

	/// The section label that stands above the first row of a section.
	private func sectionTitle(before row: BrowseRow) -> String? {
		for group in browse.groups {
			for section in group.sections where section.rows.first == row {
				// A query without section grouping has no label to show.
				return section.title.map { "\($0) · \(section.rows.count)" }
			}
		}
		return nil
	}
}

// MARK: Presets and query

private struct PresetRow: View {
	let app: AppModel
	let browse: BrowseModel
	@State private var isNaming = false
	@State private var presetName = ""
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		ScrollView(.horizontal, showsIndicators: false) {
			HStack(spacing: Tokens.Spacing.space6) {
				HomeEyebrow(text: "Presets")
					.padding(.trailing, Tokens.Spacing.space4)
				ForEach(browse.presets) { preset in
					let isActive = browse.activePreset == preset
					Button {
						browse.apply(preset)
						app.syncTodayItemCount()
					} label: {
						Text(chipLabel(preset, isActive: isActive))
							.textStyle(Tokens.Typography.callout).fontWeight(.semibold)
							.foregroundStyle(isActive ? Tokens.Colors.onPrimary : Tokens.Colors.inkSecondary)
							.padding(.horizontal, 10)
							.padding(.vertical, 4)
							.background(isActive ? Tokens.Colors.primary : Tokens.Colors.fillControl, in: Capsule())
					}
					.buttonStyle(.plain)
					.help(preset.query)
					.contextMenu {
						if !preset.isBuiltIn {
							Button("Remove preset") { browse.removePreset(preset) }
						}
					}
				}
				Button {
					isNaming = true
				} label: {
					Label("Save current query as preset", systemImage: "plus")
						.labelStyle(TightLabelStyle())
						.textStyle(Tokens.Typography.callout)
						.foregroundStyle(Tokens.Colors.inkMuted)
						.padding(.horizontal, 8)
						.padding(.vertical, 4)
				}
				.buttonStyle(.plain)
				.disabled(browse.queryMessage != nil)
				.popover(isPresented: $isNaming, arrowEdge: .bottom) {
					HStack {
						TextField("Preset name", text: $presetName)
							.textFieldStyle(.roundedBorder)
							.frame(width: 180)
							.onSubmit(save)
						Button("Save", action: save)
							.buttonStyle(.borderedProminent)
							.tint(accent)
							.disabled(presetName.trimmingCharacters(in: .whitespaces).isEmpty)
					}
					.padding(Tokens.Spacing.space12)
					.onAppear { app.setTextField("preset-name", isFocused: true) }
					.onDisappear { app.setTextField("preset-name", isFocused: false) }
				}
			}
		}
	}

	/// "Backlog · 18"; the active chip leaves its count to the summary line, as the board draws it.
	private func chipLabel(_ preset: BrowsePreset, isActive: Bool) -> String {
		guard !isActive, let count = browse.count(of: preset) else { return preset.name }
		return "\(preset.name) · \(count)"
	}

	private func save() {
		if browse.savePreset(named: presetName) {
			presetName = ""
			isNaming = false
		}
	}
}

private struct QueryLine: View {
	let app: AppModel
	let browse: BrowseModel
	@FocusState private var isFocused: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			HStack(spacing: Tokens.Spacing.space8) {
				Text("/")
					.textStyle(Tokens.Typography.mono)
					.foregroundStyle(Tokens.Colors.inkMuted)
				TextField("status:open story:WEB-142 due:<=week section:pending estimate:none", text: Bindable(browse).queryText)
					.textFieldStyle(.plain)
					.textStyle(Tokens.Typography.mono)
					.foregroundStyle(Tokens.Colors.ink)
					.focused($isFocused)
					.onSubmit {
						browse.submitQuery()
						app.syncTodayItemCount()
						// Return leaves the field, so the keys walk the result.
						isFocused = false
					}
					.onExitCommand {
						isFocused = false
					}
				if !browse.queryText.isEmpty {
					Button {
						browse.queryText = ""
						browse.submitQuery()
						app.syncTodayItemCount()
					} label: {
						Image(systemName: "xmark.circle.fill")
							.foregroundStyle(Tokens.Colors.lineControl)
					}
					.buttonStyle(.plain)
					.accessibilityLabel("Clear the query")
				}
				Text("\(browse.taskCount) match\(browse.taskCount == 1 ? "" : "es")")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			.padding(.horizontal, 10)
			.frame(height: Tokens.Size.sizeControl)
			.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
			.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd).strokeBorder(browse.queryMessage == nil ? Tokens.Colors.line : Tokens.Colors.danger))
			if let message = browse.queryMessage {
				Text(message)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.danger)
			}
		}
		.onChange(of: app.browseQueryFocusRequest) { isFocused = true }
		.onChange(of: isFocused) { app.setTextField("browse-query", isFocused: isFocused) }
		.onDisappear { app.setTextField("browse-query", isFocused: false) }
	}
}

// MARK: Table

private struct TableHeader: View {
	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Text("Task").frame(maxWidth: .infinity, alignment: .leading)
			Text("Waiting on").frame(width: BrowseColumns.waiting, alignment: .leading)
			Text("Scheduled").frame(width: BrowseColumns.scheduled, alignment: .leading)
			Text("Due").frame(width: BrowseColumns.due, alignment: .leading)
			Text("Est").frame(width: BrowseColumns.estimate, alignment: .trailing)
			Text("Story").frame(width: BrowseColumns.story, alignment: .leading)
			Text("Id").frame(width: BrowseColumns.id, alignment: .leading)
		}
		.textStyle(Tokens.Typography.eyebrow)
		.textCase(.uppercase)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.padding(.horizontal, 10)
		.padding(.vertical, Tokens.Spacing.space4)
		.overlay(alignment: .bottom) { Rectangle().fill(Tokens.Colors.line).frame(height: 1) }
		.accessibilityHidden(true)
	}
}

private struct GroupHeader: View {
	let app: AppModel
	let browse: BrowseModel
	let group: BrowseStoryGroup
	let isSelected: Bool
	let index: Int
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		let isFolded = browse.isFolded(group)
		HStack(spacing: Tokens.Spacing.space8) {
			Image(systemName: isFolded ? "chevron.right" : "chevron.down")
				.iconGlyph(.tag, weight: .semibold)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.frame(width: 14)
			if let number = group.storyNumber {
				MonoChip(text: number)
			}
			Text(group.title)
				.textStyle(Tokens.Typography.headline)
				.foregroundStyle(Tokens.Colors.ink)
				.lineLimit(1)
			Text(isFolded ? "\(group.countLabel) · folded" : group.countLabel)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkMuted)
			Spacer()
		}
		.padding(.horizontal, 10)
		.padding(.top, 14)
		.padding(.bottom, Tokens.Spacing.space6)
		.focusBorder(isSelected, cornerRadius: Tokens.Radius.radiusMd)
		.contentShape(Rectangle())
		.onTapGesture {
			app.focus.select(index, in: .today)
			browse.toggleFold(group)
			app.syncTodayItemCount()
		}
		.peekPopover(app: app, subject: group.storyNumber.map(PeekSubject.story))
		.accessibilityElement(children: .combine)
		.accessibilityLabel("\(group.title), \(group.countLabel)")
		.accessibilityAddTraits(isSelected ? .isSelected : [])
		.accessibilityHint(isFolded ? "Folded" : "Open")
	}
}

private struct SectionLabel: View {
	let text: String

	var body: some View {
		HomeEyebrow(text: text)
			.padding(EdgeInsets(top: 6, leading: 34, bottom: 3, trailing: 10))
	}
}

private struct TaskRow: View {
	let app: AppModel
	let row: BrowseRow
	let isSelected: Bool
	let index: Int
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space8) {
				Button {
					app.completeBrowseTask(row.id)
				} label: {
					Image(systemName: row.isDone ? "checkmark.square.fill" : "square")
						.iconGlyph(.menu)
						.foregroundStyle(row.isDone ? accent : Tokens.Colors.lineControl)
				}
				.buttonStyle(.plain)
				.disabled(row.isDone)
				.accessibilityLabel("Done: \(row.text)")
				Text(row.text)
					.fontWeight(row.state == .inProgress ? .semibold : .regular)
					.strikethrough(row.isDone)
					.foregroundStyle(row.isDone ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
					.lineLimit(2)
					.frame(maxWidth: .infinity, alignment: .leading)
			}
			.padding(.leading, 20)
			Group {
				if let who = row.waitingOn {
					CautionChip(text: row.waitingDays.map { "\(who) · \($0)d" } ?? who)
				} else {
					dash
				}
			}
			.frame(width: BrowseColumns.waiting, alignment: .leading)
			cell(row.scheduledLabel)
				.frame(width: BrowseColumns.scheduled, alignment: .leading)
			cell(row.dueLabel, color: row.isOverdue ? Tokens.Colors.danger : Tokens.Colors.inkSecondary)
				.frame(width: BrowseColumns.due, alignment: .leading)
			cell(row.estimateLabel)
				.frame(width: BrowseColumns.estimate, alignment: .trailing)
			Group {
				if let number = row.storyNumber {
					Text(number)
						.textStyle(Tokens.Typography.mono)
						.foregroundStyle(Tokens.Colors.inkMuted)
				} else {
					dash
				}
			}
			.frame(width: BrowseColumns.story, alignment: .leading)
			Text(row.id)
				.textStyle(Tokens.Typography.mono)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.frame(width: BrowseColumns.id, alignment: .leading)
		}
		.textStyle(Tokens.Typography.body)
		.padding(.horizontal, 10)
		.padding(.vertical, 5)
		.focusBorder(isSelected, cornerRadius: Tokens.Radius.radiusMd)
		.contentShape(Rectangle())
		.onTapGesture(count: 2) { app.openPreview(taskId: row.id) }
		.onTapGesture { app.focus.select(index, in: .today) }
		.peekPopover(app: app, subject: .task(row.id))
		.accessibilityElement(children: .contain)
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}

	private var dash: some View {
		Text("—").foregroundStyle(Tokens.Colors.inkDisabled)
	}

	@ViewBuilder
	private func cell(_ text: String?, color: Color = Tokens.Colors.inkSecondary) -> some View {
		if let text {
			Text(text)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(color)
				.monospacedDigit()
		} else {
			dash
		}
	}
}
