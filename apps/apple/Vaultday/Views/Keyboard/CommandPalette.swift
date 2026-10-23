import SwiftUI
import VaultdayCore

/// The command palette (⌘K): a field over the sections the palette model
/// renders from Keymap.yaml, recent stories and workspaces first. ↑ ↓ walk
/// the rows, ⏎ runs one, Esc closes.
struct CommandPalette: View {
	let app: AppModel
	@Bindable var palette: CommandPaletteModel
	@FocusState private var isFocused: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: 2) {
			TextField("Type a command, a story or a workspace…", text: $palette.query)
				.textFieldStyle(.plain)
				.textStyle(Tokens.Typography.headline).fontWeight(.regular)
				.focused($isFocused)
				.padding(.horizontal, 10)
				.frame(height: 36)
				.onSubmit { app.choosePaletteRow() }
				.onExitCommand { palette.close() }
				.onKeyPress(.downArrow) {
					palette.moveSelection(by: 1)
					return .handled
				}
				.onKeyPress(.upArrow) {
					palette.moveSelection(by: -1)
					return .handled
				}
			Rectangle().fill(Tokens.Colors.line).frame(height: 1)
			ScrollViewReader { proxy in
				ScrollView {
					LazyVStack(alignment: .leading, spacing: 2) {
						ForEach(palette.sections) { section in
							Text(section.title)
								.textStyle(Tokens.Typography.eyebrow)
								.textCase(.uppercase)
								.foregroundStyle(Tokens.Colors.inkMuted)
								.padding(EdgeInsets(top: 6, leading: 10, bottom: 2, trailing: 10))
							ForEach(section.rows) { row in
								PaletteRowView(row: row, isSelected: palette.selectedRow?.id == row.id) {
									app.choosePaletteRow(row)
								}
								.id(row.id)
							}
						}
						if palette.rows.isEmpty {
							Text("Nothing matches")
								.foregroundStyle(Tokens.Colors.inkMuted)
								.padding(10)
						}
					}
				}
				.onChange(of: palette.selectedIndex) {
					if let id = palette.selectedRow?.id { proxy.scrollTo(id) }
				}
			}
			.frame(maxHeight: 420)
		}
		.padding(10)
		.frame(width: 480)
		.background(Tokens.Colors.surface, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
		.shadow(color: .black.opacity(0.2), radius: 20, y: 12)
		.onAppear { isFocused = true }
		.onChange(of: isFocused) { app.setTextField("palette", isFocused: isFocused) }
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Command palette")
	}
}

private struct PaletteRowView: View {
	let row: PaletteRow
	let isSelected: Bool
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: Tokens.Spacing.space8) {
				MarkIcon(mark: TypeMark(paletteChoice: row.choice).mark, size: 12)
				Text(row.title)
					.frame(maxWidth: .infinity, alignment: .leading)
					.lineLimit(1)
				if let detail = row.detail {
					Text(detail)
						.textStyle(Tokens.Typography.mono)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
				ForEach(Array(row.caps.enumerated()), id: \.offset) { _, caps in
					SequenceCaps(caps: caps)
				}
				// ⏎ draws on beside the highlighted row as the highlight moves.
				IconPresence(isShown: isSelected) {
					Image(systemName: "return")
						.iconGlyph(.tag, weight: .semibold)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
				.frame(width: 14)
				.accessibilityHidden(true)
			}
			.textStyle(Tokens.Typography.body)
			.foregroundStyle(Tokens.Colors.ink)
			.padding(.horizontal, 10)
			.padding(.vertical, 6)
			// Keyboard focus is the thick border, never a tinted row (feedback item 34).
			.focusBorder(isSelected, cornerRadius: Tokens.Radius.radiusSm)
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}
}
