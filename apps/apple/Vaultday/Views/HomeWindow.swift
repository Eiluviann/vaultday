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
	@State private var forceClick = ForceClickMonitor()
	@State private var leftOfOne = LeftOfOneMonitor()

	var body: some View {
		VStack(spacing: 0) {
			HeaderBar(app: app)
			if app.screen == .story {
				StoryScreen(app: app, story: app.story)
					.frame(maxHeight: .infinity)
			} else {
				homeColumns
			}
		}
		.background(Tokens.Colors.surfaceWindow)
		.environment(\.workspaceAccent, app.accentColor)
		.environment(\.screenShare, app.screenShare)
		// The task preview slides over the Inbox and the side panel from the right edge.
		.overlay(alignment: .trailing) {
			if let content = app.preview.content {
				TaskPreviewPanel(app: app, content: content)
					.frame(width: HomeLayout.inboxWidth + HomeLayout.sidePanelWidth)
					.padding(.top, HomeLayout.headerHeight)
					.transition(.move(edge: .trailing))
			} else if let card = app.inbox.previewCard {
				SignalPreviewPanel(app: app, card: card)
					.frame(width: HomeLayout.inboxWidth + HomeLayout.sidePanelWidth)
					.padding(.top, HomeLayout.headerHeight)
					.transition(.move(edge: .trailing))
			}
		}
		.animation(.easeOut(duration: Tokens.Motion.motionTransition), value: app.preview.isOpen)
		.animation(.easeOut(duration: Tokens.Motion.motionTransition), value: app.inbox.previewId)
		.ignoresSafeArea(.container, edges: .top)
		.focusable()
		.focusEffectDisabled()
		.focused($hasKeyFocus)
		// The list takes the keys at launch, not the first text field.
		.defaultFocus($hasKeyFocus, true)
		.onKeyPress(phases: .down) { press in
			let start = ProcessInfo.processInfo.systemUptime
			let stroke = KeyStroke(press)
			guard app.handle(stroke, isTyping: isTypingInTextField) else { return .ignored }
			KeyLatency.mark(stroke, event: NSApp.currentEvent, handlerStart: start)
			return .handled
		}
		.overlay(alignment: .bottomTrailing) {
			ShowCommand(dispatcher: app.dispatcher)
		}
		.overlay(alignment: .bottomLeading) {
			RegisterNote(register: app.keyboard.register)
				.padding(.leading, app.focus.isSidebarCollapsed ? HomeLayout.railWidth : HomeLayout.sidebarWidth)
		}
		// The palette and the change editor float over the window's top third.
		.overlay(alignment: .top) {
			if app.palette.isOpen {
				CommandPalette(app: app, palette: app.palette)
					.padding(.top, HomeLayout.headerHeight + 60)
			} else if let request = app.keyboard.changeRequest {
				ChangeFieldEditor(app: app, request: request)
					.padding(.top, HomeLayout.headerHeight + 60)
			}
		}
		.overlay(alignment: .topTrailing) {
			if let alert = app.closeOutDue.alert {
				CloseOutAlertBanner(alert: alert, open: { app.openCloseOut() }, dismiss: { app.closeOutDue.dismissAlert() })
					.padding(.top, HomeLayout.headerHeight + Tokens.Spacing.space8)
					.padding(.trailing, Tokens.Spacing.space16)
			}
		}
		.sheet(isPresented: Binding(get: { app.closeOut.isPresented }, set: { app.closeOut.isPresented = $0 }), onDismiss: { app.refreshCloseOut() }) {
			CloseOutScreen(model: app.closeOut)
				.mainWindowChildSheet()
		}
		.onChange(of: app.palette.isOpen) { if !app.palette.isOpen { hasKeyFocus = true } }
		.onChange(of: app.keyboard.changeRequest) { if app.keyboard.changeRequest == nil { hasKeyFocus = true } }
		// Esc out of an input: its pane takes the keys back at once, so hjkl
		// act on the next stroke. A field handing focus straight to another
		// field keeps it.
		.onChange(of: app.focus.input) {
			guard app.focus.input == nil else { return }
			Task { @MainActor in
				if app.focus.input == nil, !app.palette.isOpen, !isTypingInTextField { hasKeyFocus = true }
			}
		}
		.onAppear {
			hasKeyFocus = true
			forceClick.start(app: app)
			leftOfOne.start(app: app)
		}
		.onDisappear {
			forceClick.stop()
			leftOfOne.stop()
		}
		.onChange(of: app.homeSettingsSignature) { app.reloadWorkspaceContent() }
		.task {
			// The side panel's clock: the now line, ended meetings, the Reminders intake.
			while !Task.isCancelled {
				try? await Task.sleep(for: .seconds(60))
				await app.minuteTick()
			}
		}
	}

	/// The day: sidebar (or its rail), Today (or every workspace, or Browse),
	/// Inbox and the side panel.
	private var homeColumns: some View {
		HStack(spacing: 0) {
			Group {
				if app.focus.isSidebarCollapsed {
					SidebarRail(app: app)
				} else {
					SidebarColumn(app: app)
				}
			}
			.paneNumber(.sidebar, dispatcher: app.dispatcher)
			if app.screen == .browse, !app.isShowingAll {
				BrowseColumn(app: app, browse: app.browse)
					.frame(minWidth: HomeLayout.todayMinWidth, maxWidth: .infinity)
					.paneNumber(.today, dispatcher: app.dispatcher)
			} else if app.screen == .triage {
				TriageScreen(app: app)
					.frame(minWidth: HomeLayout.todayMinWidth, maxWidth: .infinity)
					.paneNumber(.inbox, dispatcher: app.dispatcher)
			} else {
				Group {
					if app.isShowingAll, let all = app.allWorkspaces {
						AllWorkspacesColumn(app: app, all: all)
					} else {
						TodayColumn(app: app)
					}
				}
				.frame(minWidth: HomeLayout.todayMinWidth, maxWidth: .infinity)
				.paneNumber(.today, dispatcher: app.dispatcher)
				InboxColumn(app: app)
					.frame(width: HomeLayout.inboxWidth)
					.paneNumber(.inbox, dispatcher: app.dispatcher)
				SidePanelColumn(app: app)
					.frame(width: HomeLayout.sidePanelWidth)
					.paneNumber(.sidePanel, dispatcher: app.dispatcher)
			}
		}
		.frame(maxHeight: .infinity)
		// The row paints the sidebar's column to the window's foot, as the board's gradient does.
		.background(alignment: .leading) {
			Tokens.Colors.surfaceSidebar
				.frame(width: app.focus.isSidebarCollapsed ? HomeLayout.railWidth : HomeLayout.sidebarWidth)
		}
	}
}

/// ⌘ and the key left of 1, by key code: § on an ISO keyboard, ` on ANSI.
/// AppKit hands such a key to the menus before any view, and SwiftUI's key
/// press carries the character, not the key, so a monitor catches it first.
@MainActor
final class LeftOfOneMonitor {
	private var monitor: Any?

	func start(app: AppModel) {
		guard monitor == nil else { return }
		KeyStroke.leftOfOneCap = AppModel.leftOfOneCap
		monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak app] event in
			// Only a Bool crosses back out of the actor; the event stays on this thread.
			let isConsumed = MainActor.assumeIsolated {
				guard let app, let stroke = AppModel.leftOfOneStroke(event), app.mainWindow.root == .home else { return false }
				let start = ProcessInfo.processInfo.systemUptime
				guard app.handle(stroke) else { return false }
				KeyLatency.mark(stroke, event: event, handlerStart: start)
				return true
			}
			return isConsumed ? nil : event
		}
	}

	func stop() {
		if let monitor { NSEvent.removeMonitor(monitor) }
		monitor = nil
	}
}

/// Whether a text field is being typed in: AppKit's field editor holds a
/// window's first responder then, whichever field it is, Adjust's included.
@MainActor
var isTypingInTextField: Bool {
	NSApp.windows.contains { $0.isVisible && $0.firstResponder is NSText }
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
			.animation(.easeOut(duration: Tokens.Motion.motionWash), value: isFocused)
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
		case .pageUp: "pageup"
		case .pageDown: "pagedown"
		case .delete: "delete"
		default: String(press.key.character).lowercased()
		}
		self.init(key: key, modifiers: modifiers)
	}
}

extension KeyboardShortcut {
	/// A menu item's key equivalent for a Keymap.yaml stroke, such as ⌘, for Settings.
	init?(_ stroke: KeyStroke) {
		let key: KeyEquivalent? = switch stroke.key {
		case "return": .return
		case "escape": .escape
		case "tab": .tab
		case "space": .space
		case "up": .upArrow
		case "down": .downArrow
		case "left": .leftArrow
		case "right": .rightArrow
		case "pageup": .pageUp
		case "pagedown": .pageDown
		case "delete": .delete
		default: stroke.key.count == 1 ? stroke.key.first.map { KeyEquivalent($0) } : nil
		}
		guard let key else { return nil }
		var modifiers: EventModifiers = []
		if stroke.modifiers.contains(.shift) { modifiers.insert(.shift) }
		if stroke.modifiers.contains(.control) { modifiers.insert(.control) }
		if stroke.modifiers.contains(.option) { modifiers.insert(.option) }
		if stroke.modifiers.contains(.command) { modifiers.insert(.command) }
		self.init(key, modifiers: modifiers)
	}
}
