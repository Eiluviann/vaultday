import SwiftUI
import VaultdayCore

/// The side panel's Day log tab: the day's log lines, the kind chips, and the
/// field that appends a line to today's log through the core.
struct DayLogTab: View {
	let app: AppModel
	let model: DayLogModel
	@Environment(\.workspaceAccent) private var accent
	@FocusState private var isAppendFocused: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space6) {
				Text(model.caption)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.lineLimit(1)
				Spacer()
				HeaderIconButton(symbol: "chevron.left", label: "Previous day", help: "Previous day") {
					model.showDay(offset: -1)
					app.syncSidePanelItemCount()
				}
				Text(DayLogTab.dayLabel(model.date))
					.textStyle(Tokens.Typography.callout).fontWeight(.semibold)
					.foregroundStyle(Tokens.Colors.ink)
					.monospacedDigit()
				HeaderIconButton(symbol: "chevron.right", label: "Next day", help: "Next day") {
					model.showDay(offset: 1)
					app.syncSidePanelItemCount()
				}
			}
			chips
			list
			append
		}
		.onChange(of: model.kindFilter) { app.syncSidePanelItemCount() }
		.onChange(of: model.lines) { app.syncSidePanelItemCount() }
		.onChange(of: isAppendFocused) { app.setTextField("day-log-append", isFocused: isAppendFocused) }
		.onDisappear { app.setTextField("day-log-append", isFocused: false) }
		.onAppear { app.syncSidePanelItemCount() }
	}

	/// "Thu 15 Oct", or the date as written when it cannot be read.
	static func dayLabel(_ iso: String) -> String {
		DayText.date(from: iso).map(DayText.chip) ?? iso
	}

	private var chips: some View {
		ScrollView(.horizontal, showsIndicators: false) {
			HStack(spacing: Tokens.Spacing.space4) {
				chip("All", isActive: model.kindFilter == nil) { model.kindFilter = nil }
				ForEach(DayLogKind.allCases, id: \.self) { kind in
					chip(kind.title, isActive: model.kindFilter == kind, tint: Self.tint(kind)) { model.kindFilter = kind }
				}
			}
		}
	}

	private func chip(_ title: String, isActive: Bool, tint: Color = Tokens.Colors.inkSecondary, action: @escaping () -> Void) -> some View {
		Button(action: action) {
			Text(title)
				.textStyle(Tokens.Typography.callout).fontWeight(.semibold)
				.foregroundStyle(isActive ? Tokens.Colors.onPrimary : tint)
				.padding(.horizontal, 10)
				.padding(.vertical, 4)
				.background(isActive ? Tokens.Colors.primary : Tokens.Colors.surfaceRaised, in: Capsule())
		}
		.buttonStyle(.plain)
		.accessibilityAddTraits(isActive ? .isSelected : [])
	}

	private var list: some View {
		ScrollView {
			LazyVStack(alignment: .leading, spacing: 0) {
				if model.visibleLines.isEmpty {
					EmptyState(icon: "doc.text", text: model.lines.isEmpty ? "Nothing logged on this day" : "No \(model.kindFilter?.rawValue ?? "") lines")
				}
				ForEach(Array(model.visibleLines.enumerated()), id: \.element.id) { index, line in
					LogRow(
						app: app,
						line: line,
						isSelected: app.focus.focusedPane == .sidePanel && app.focus.selection(in: .sidePanel) == index,
						index: index,
					)
				}
			}
			.padding(.horizontal, 12)
			.padding(.vertical, Tokens.Spacing.space4)
		}
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.frame(maxHeight: .infinity)
	}

	private var append: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			HStack(spacing: Tokens.Spacing.space8) {
				VimTextField("Append to today's log", text: Bindable(model).draft, focusRequest: app.dayLogFocusRequest)
					.textFieldStyle(.plain)
					.focused($isAppendFocused)
					.padding(.horizontal, 10)
					.frame(height: 30)
					.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
					.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd).strokeBorder(Tokens.Colors.line))
					.vimModeLineBelow()
					.onSubmit(submit)
				Button("Add", action: submit)
					.buttonStyle(.borderedProminent)
					.tint(accent)
					.disabled(model.draft.trimmingCharacters(in: .whitespaces).isEmpty)
			}
			if let message = model.errorMessage {
				Text(message)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.danger)
			}
			Text("Shorthand sets the kind: decision: blocked: note: · the selected task gives the story and the id · written as a plain line, no markers")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
	}

	private func submit() {
		app.appendDayLog()
	}

	static func tint(_ kind: DayLogKind) -> Color {
		switch kind {
		case .decision: Tokens.Colors.ai
		case .blocker: Tokens.Colors.caution
		default: Tokens.Colors.inkSecondary
		}
	}
}

private struct LogRow: View {
	let app: AppModel
	let line: DayLogLine
	let isSelected: Bool
	let index: Int
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		HStack(alignment: .top, spacing: 10) {
			Text(line.time)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.monospacedDigit()
				.frame(width: 40, alignment: .leading)
				.padding(.top, 2)
			Image(systemName: mark.symbol)
				.iconGlyph(.row)
				.foregroundStyle(mark.tone.color)
				.frame(width: 16)
				.padding(.top, 1)
				.accessibilityLabel(mark.name)
			VStack(alignment: .leading, spacing: 1) {
				Text(line.text)
					.textStyle(Tokens.Typography.body)
					.foregroundStyle(Tokens.Colors.ink)
					.fixedSize(horizontal: false, vertical: true)
				Text(detail)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(line.agent == nil ? Tokens.Colors.inkMuted : Tokens.Colors.ai)
			}
			.frame(maxWidth: .infinity, alignment: .leading)
		}
		.padding(.vertical, 8)
		.padding(.horizontal, 4)
		.overlay(alignment: .bottom) { Rectangle().fill(Tokens.Colors.line).frame(height: 1) }
		.focusBorder(isSelected, cornerRadius: Tokens.Radius.radiusMd)
		.contentShape(Rectangle())
		.onTapGesture { app.focus.select(index, in: .sidePanel) }
		.peekPopover(app: app, subject: .logLine(line))
		.accessibilityElement(children: .combine)
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}

	private var detail: String {
		[line.kind, line.agent, line.story, line.task].compactMap(\.self).joined(separator: " · ")
	}

	/// The line's kind as its one icon and colour.
	private var mark: IconMark {
		TypeMark(dayLogKind: line.kind, isByModel: line.agent != nil).mark
	}
}
