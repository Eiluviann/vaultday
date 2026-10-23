import SwiftUI
import VaultdayCore

/// Proportions of the Mac Home board (mockup "Mac · Home, base, sample
/// data", 1440 × 900): a 38 px header over four columns.
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

/// The Home window: header, sidebar (or its rail), Today (or every
/// workspace together), Inbox and the side panel.
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
				Group {
					if app.isShowingAll, let all = app.allWorkspaces {
						AllWorkspacesColumn(app: app, all: all)
					} else {
						TodayColumn(app: app)
					}
				}
				.frame(minWidth: HomeLayout.todayMinWidth, maxWidth: .infinity)
				InboxColumn(app: app)
					.frame(width: HomeLayout.inboxWidth)
				SidePanelColumn(app: app)
					.frame(width: HomeLayout.sidePanelWidth)
			}
			.frame(maxHeight: .infinity)
			// The row paints the sidebar's column to the window's foot, as the board's gradient does.
			.background(alignment: .leading) {
				Tokens.Colors.surfaceSidebar
					.frame(width: app.focus.isSidebarCollapsed ? HomeLayout.railWidth : HomeLayout.sidebarWidth)
			}
		}
		.background(Tokens.Colors.surfaceWindow)
		.environment(\.workspaceAccent, app.accentColor)
		.ignoresSafeArea(.container, edges: .top)
		.focusable()
		.focusEffectDisabled()
		.focused($hasKeyFocus)
		// The list takes the keys at launch, not the first text field.
		.defaultFocus($hasKeyFocus, true)
		.onKeyPress(phases: .down) { press in
			app.handle(KeyStroke(press), isTyping: isTypingInTextField) ? .handled : .ignored
		}
		.overlay(alignment: .bottomTrailing) {
			ShowCommand(text: app.dispatcher.pendingText)
		}
		.onAppear { hasKeyFocus = true }
		.onChange(of: app.homeSettingsSignature) { app.reloadWorkspaceContent() }
		.task {
			// The side panel's clock: the now line, ended meetings, the Reminders intake.
			while !Task.isCancelled {
				try? await Task.sleep(for: .seconds(60))
				await app.minuteTick()
			}
		}
	}
}

/// Whether a text field is being typed in: AppKit's field editor holds a
/// window's first responder then, whichever field it is, Adjust's included.
@MainActor
private var isTypingInTextField: Bool {
	NSApp.windows.contains { $0.isVisible && $0.firstResponder is NSText }
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

extension EnvironmentValues {
	/// The open workspace's identity color: the Add button, the selected switcher row, the focused pane's wash.
	@Entry var workspaceAccent: Color = Tokens.Colors.wsBlue
}

/// A pane's ground: its own color, washed with the workspace color while the pane has keyboard focus.
struct PaneGround: ViewModifier {
	let ground: Color
	let isFocused: Bool
	@Environment(\.workspaceAccent) private var accent

	func body(content: Content) -> some View {
		content.background {
			ZStack {
				ground
				if isFocused {
					accent.opacity(0.09)
				}
			}
			.animation(.easeOut(duration: 0.15), value: isFocused)
		}
	}
}

extension View {
	/// The pane's ground and its focus wash.
	func paneGround(_ ground: Color, isFocused: Bool) -> some View {
		modifier(PaneGround(ground: ground, isFocused: isFocused))
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
