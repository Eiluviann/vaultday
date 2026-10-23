import SwiftUI
import VaultdayCore

/// Proportions of the Mac Home board (mockup "Mac · Home, base without AI",
/// 1440 × 900): a 38 px header over four columns.
enum HomeLayout {
	static let headerHeight: CGFloat = 38
	static let sidebarWidth: CGFloat = 220
	static let railWidth: CGFloat = 44
	static let todayMinWidth: CGFloat = 420
	static let inboxWidth: CGFloat = 300
	static let sidePanelWidth: CGFloat = 400
	static let sidePanelRailWidth: CGFloat = 44
	/// Room the window's traffic lights take at the header's leading edge.
	static let trafficLightsWidth: CGFloat = 70
}

/// The Home window: header, sidebar (or its rail), Today, Inbox and the side
/// panel. Content stays the spike's Today list; Home itself is PR 8.
struct HomeWindow: View {
	@Bindable var app: AppModel
	@FocusState private var hasKeyFocus: Bool

	var body: some View {
		VStack(spacing: 0) {
			HeaderBar(app: app)
			HStack(spacing: 0) {
				if app.focus.isSidebarCollapsed {
					SidebarRail(app: app)
				} else {
					SidebarColumn(app: app)
				}
				TodayColumn(app: app)
					.frame(minWidth: HomeLayout.todayMinWidth, maxWidth: .infinity)
				InboxColumn(app: app)
					.frame(width: HomeLayout.inboxWidth)
				SidePanelColumn(app: app)
					.frame(width: HomeLayout.sidePanelWidth)
			}
			.frame(maxHeight: .infinity)
		}
		.background(Tokens.Colors.surfaceWindow)
		.ignoresSafeArea(.container, edges: .top)
		.focusable()
		.focusEffectDisabled()
		.focused($hasKeyFocus)
		.onKeyPress(phases: .down) { press in
			app.handle(KeyStroke(press)) ? .handled : .ignored
		}
		.overlay(alignment: .bottomTrailing) {
			ShowCommand(text: app.dispatcher.pendingText)
		}
		.onAppear { hasKeyFocus = true }
	}
}

/// The pending strokes, bottom right, as vim's showcmd.
private struct ShowCommand: View {
	let text: String

	var body: some View {
		if !text.isEmpty {
			Text(text)
				.font(Tokens.Typography.kbd.font)
				.foregroundStyle(Tokens.Colors.ink)
				.padding(.horizontal, Tokens.Spacing.space8)
				.padding(.vertical, Tokens.Spacing.space4)
				.background(Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
				.padding(Tokens.Spacing.space12)
		}
	}
}

/// A pane's focus wash: the focused pane carries the focus ring color along its inner edge.
struct PaneFocusEdge: ViewModifier {
	let isFocused: Bool

	func body(content: Content) -> some View {
		content.overlay {
			if isFocused {
				Rectangle()
					.strokeBorder(Tokens.Colors.focus.opacity(0.55), lineWidth: 2)
					.allowsHitTesting(false)
			}
		}
	}
}

extension View {
	func paneFocus(_ isFocused: Bool) -> some View {
		modifier(PaneFocusEdge(isFocused: isFocused))
	}
}

extension KeyStroke {
	/// The stroke SwiftUI reports, in Keymap.yaml's names.
	init(_ press: KeyPress) {
		var modifiers: KeyModifiers = []
		if press.modifiers.contains(.shift) { modifiers.insert(.shift) }
		if press.modifiers.contains(.control) { modifiers.insert(.control) }
		if press.modifiers.contains(.option) { modifiers.insert(.option) }
		if press.modifiers.contains(.command) { modifiers.insert(.command) }
		let key: String = switch press.key {
		case .return: "return"
		case .escape: "escape"
		case .tab: "tab"
		case .space: "space"
		case .upArrow: "up"
		case .downArrow: "down"
		case .leftArrow: "left"
		case .rightArrow: "right"
		default: String(press.key.character).lowercased()
		}
		self.init(key: key, modifiers: modifiers)
	}
}
