import SwiftUI
import VaultdayCore

/// The 38 px header: room for the traffic lights, the sidebar toggle, the
/// screen-share toggle and the palette's field, whose
/// placeholder names the scope.
struct HeaderBar: View {
	let app: AppModel

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Color.clear.frame(width: HomeLayout.trafficLightsWidth)
			Button {
				app.perform(.toggleSidebar)
			} label: {
				Image(systemName: app.focus.isSidebarCollapsed ? "sidebar.squares.left" : "sidebar.left")
					.iconGlyph(.menu)
					.iconReplace(value: app.focus.isSidebarCollapsed)
					.frame(width: 24, height: 24)
			}
			.buttonStyle(.plain)
			.foregroundStyle(Tokens.Colors.inkMuted)
			.help(app.keyLabel(.toggleSidebar).map { "Toggle sidebar (\($0))" } ?? "Toggle sidebar")
			.accessibilityLabel("Toggle sidebar")
			Spacer()
			if let due = app.closeOutDue.due {
				RitualButton(due: due) { app.openCloseOut() }
			}
			ScreenShareToggle(app: app)
			SearchField(app: app)
		}
		.padding(.horizontal, Tokens.Spacing.space12)
		.frame(height: HomeLayout.headerHeight)
		.background(Tokens.Colors.surfaceSunken)
		.overlay(alignment: .bottom) {
			Rectangle().fill(Tokens.Colors.line).frame(height: 1)
		}
	}
}

/// The header's ritual slot: only the next ritual still due, with its
/// time; from Friday 14:00 that is the week close-out until it is closed.
private struct RitualButton: View {
	let due: CloseOutDue
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: Tokens.Spacing.space6) {
				Image(systemName: "checkmark.seal")
					.iconGlyph(.row)
				Text(due.title)
					.fontWeight(.semibold)
				Text(due.timeText)
					.foregroundStyle(due.isOverdue ? Tokens.Colors.danger : Tokens.Colors.inkMuted)
			}
			.textStyle(Tokens.Typography.callout)
			.foregroundStyle(Tokens.Colors.ink)
			.padding(.horizontal, Tokens.Spacing.space8)
			.frame(height: 24)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
			.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(due.isOverdue ? Tokens.Colors.danger : Tokens.Colors.lineButton))
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
		.help("Open the \(due.title.lowercased())")
		.accessibilityLabel("\(due.title), \(due.timeText)")
	}
}

/// Screen-share mode's place in the header: off, outlined; on, filled, with
/// no caption beside it, since nothing on screen may say that the mode hides
/// something. Drawn disabled, with the reason as its tooltip, while
/// All workspaces or a life workspace is shown: the mode starts from a work
/// workspace. A refused switch says why next to it for a few seconds.
private struct ScreenShareToggle: View {
	let app: AppModel

	private var isOn: Bool { app.screenShare.isOn }
	private var disabledReason: String? { app.screenShareStartDisabledReason }

	var body: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			if let notice = app.screenShare.notice {
				Text(notice)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.caution)
					.lineLimit(1)
					.task(id: notice) {
						try? await Task.sleep(for: .seconds(4))
						app.screenShare.dismissNotice()
					}
			}
			Button {
				app.toggleScreenShare()
			} label: {
				// custom symbol pending: screen-share
				Image(systemName: isOn ? "rectangle.fill.on.rectangle.fill" : "rectangle.on.rectangle")
					.iconGlyph(.row)
					.iconReplace(value: isOn)
					.foregroundStyle(isOn ? Tokens.Colors.onPrimary : disabledReason == nil ? Tokens.Colors.inkSecondary : Tokens.Colors.inkDisabled)
					.frame(width: 24, height: 24)
					.background(isOn ? Tokens.Colors.primary : Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
					.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(Tokens.Colors.lineButton))
					.contentShape(Rectangle())
			}
			.buttonStyle(.plain)
			.disabled(disabledReason != nil)
			.help(disabledReason ?? tooltip)
			.accessibilityLabel("Screen-share mode")
			.accessibilityValue(isOn ? "On" : disabledReason.map { "Off, \($0)" } ?? "Off")
			.accessibilityAddTraits(isOn ? .isSelected : [])
		}
	}

	private var tooltip: String {
		ScreenShareModel.toggleTooltip(isOn: isOn, keyLabel: app.keyLabel(.toggleScreenShare))
	}
}

/// The palette's place in the header: the scope in its placeholder; a click
/// or ⌘K opens the command palette, where commands, stories and workspaces
/// are found.
private struct SearchField: View {
	let app: AppModel

	var body: some View {
		Button {
			app.perform(.openPalette)
		} label: {
			HStack(spacing: Tokens.Spacing.space6) {
				Image(systemName: "magnifyingglass")
					.iconGlyph(.row)
				Text(placeholder)
					.frame(maxWidth: .infinity, alignment: .leading)
				ActionCap(app: app, action: .openPalette)
			}
			.textStyle(Tokens.Typography.callout)
			.foregroundStyle(Tokens.Colors.inkMuted)
			.padding(.horizontal, Tokens.Spacing.space8)
			.frame(width: 220, height: 24)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
			.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(app.palette.isOpen ? Tokens.Colors.focus : Tokens.Colors.lineButton))
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
		.accessibilityLabel("Command palette")
	}

	private var placeholder: String {
		if app.isShowingAll { return "Search everything…" }
		guard let name = app.workspace?.name, !name.isEmpty else { return "Search…" }
		return "Search \(name)…"
	}
}
