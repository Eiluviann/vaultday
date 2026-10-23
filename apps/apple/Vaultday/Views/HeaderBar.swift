import SwiftUI
import VaultdayCore

/// The 38 px header: room for the traffic lights, the sidebar toggle, and
/// the search field on the right whose placeholder names the scope.
struct HeaderBar: View {
	let app: AppModel

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Color.clear.frame(width: HomeLayout.trafficLightsWidth)
			Button {
				app.toggleSidebar()
			} label: {
				Image(systemName: "sidebar.left")
					.frame(width: 24, height: 24)
			}
			.buttonStyle(.plain)
			.foregroundStyle(Tokens.Colors.inkSecondary)
			.help("Toggle sidebar (⌃⌘S)")
			Spacer()
			SearchPlaceholder(scope: app.workspace?.name ?? "")
		}
		.padding(.horizontal, Tokens.Spacing.space12)
		.frame(height: HomeLayout.headerHeight)
		.background(Tokens.Colors.surfaceSunken)
		.overlay(alignment: .bottom) {
			Rectangle().fill(Tokens.Colors.line).frame(height: 1)
		}
	}
}

/// The search field's place and look; search itself arrives with the palette.
private struct SearchPlaceholder: View {
	let scope: String

	var body: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			Image(systemName: "magnifyingglass")
			Text(scope.isEmpty ? "Search…" : "Search \(scope)…")
			Spacer()
			Text("⌘K")
				.font(Tokens.Typography.kbd.font)
				.padding(.horizontal, Tokens.Spacing.space4)
				.background(Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
		}
		.font(Tokens.Typography.callout.font)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.padding(.horizontal, Tokens.Spacing.space8)
		.frame(width: 220, height: 24)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
	}
}
