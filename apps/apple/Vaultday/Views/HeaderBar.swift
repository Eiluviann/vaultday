import SwiftUI
import VaultdayCore

/// The 38 px header: room for the traffic lights, the sidebar toggle, the
/// screen-share toggle (a later version) and the search field whose
/// placeholder names the scope. Until the palette arrives, ⌘K focuses the field.
struct HeaderBar: View {
	let app: AppModel

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Color.clear.frame(width: HomeLayout.trafficLightsWidth)
			Button {
				app.perform(.toggleSidebar)
			} label: {
				Image(systemName: "sidebar.left")
					.font(.system(size: 14))
					.frame(width: 24, height: 24)
			}
			.buttonStyle(.plain)
			.foregroundStyle(Tokens.Colors.inkMuted)
			.help(app.keyLabel(.toggleSidebar).map { "Toggle sidebar (\($0))" } ?? "Toggle sidebar")
			.accessibilityLabel("Toggle sidebar")
			Spacer()
			ScreenShareToggle()
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

/// Screen-share mode's place in the header; the mode itself is a later version.
private struct ScreenShareToggle: View {
	var body: some View {
		Image(systemName: "rectangle.on.rectangle")
			.font(.system(size: 13))
			.foregroundStyle(Tokens.Colors.inkDisabled)
			.frame(width: 24, height: 24)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
			.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(Tokens.Colors.lineButton))
			.contentShape(Rectangle())
			.help("Available in a later version")
			.accessibilityElement()
			.accessibilityLabel("Screen-share mode")
			.accessibilityValue("Available in a later version")
			.accessibilityAddTraits(.isButton)
	}
}

/// The search field: the scope in its placeholder, ⌘K to focus it. Search
/// itself arrives with the palette.
private struct SearchField: View {
	let app: AppModel
	@State private var text = ""
	@FocusState private var isFocused: Bool

	var body: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			Image(systemName: "magnifyingglass")
				.font(.system(size: 11))
			TextField(placeholder, text: $text)
				.textFieldStyle(.plain)
				.focused($isFocused)
				.onExitCommand { isFocused = false }
			ActionCap(app: app, action: .focusSearch)
		}
		.font(Tokens.Typography.callout.font)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.padding(.horizontal, Tokens.Spacing.space8)
		.frame(width: 220, height: 24)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
		.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(isFocused ? Tokens.Colors.focus : Tokens.Colors.lineButton))
		.onChange(of: app.searchFocusRequest) { isFocused = true }
		.onChange(of: isFocused) { app.setTextField("search", isFocused: isFocused) }
	}

	private var placeholder: String {
		if app.isShowingAll { return "Search everything…" }
		guard let name = app.workspace?.name, !name.isEmpty else { return "Search…" }
		return "Search \(name)…"
	}
}
