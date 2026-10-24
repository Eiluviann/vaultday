import AppKit
import SwiftUI
import VaultdayCore

// Vim motions in the setup flow. The shared setup controls (buttons, choice
// cards, chips, segments, switches, menus, fields) mark themselves as
// options of the setup's keyboard model with `setupKeyOption`; the model
// walks them by place, and ⏎ presses the highlighted one. Under Standard the
// model is absent and the controls are as before.

extension EnvironmentValues {
	/// The setup's keyboard model while the setup runs under Vim motions; `nil` otherwise.
	@Entry var setupKeyboard: SetupKeyboardModel?
	/// Whether controls here are options of the shown step (the step list's own controls are not).
	@Entry var registersSetupKeyOptions = true
}

/// What ⏎ does on each option, by its id: its own action, or a click on it.
@MainActor
final class SetupOptionActions {
	static let shared = SetupOptionActions()

	private var actions: [UUID: () -> Void] = [:]

	func set(_ id: UUID, _ action: @escaping () -> Void) {
		actions[id] = action
	}

	func remove(_ id: UUID) {
		actions[id] = nil
	}

	func run(_ id: UUID) {
		actions[id]?()
	}

	/// A click at `point` in SwiftUI's global space (the key window's top left
	/// is its origin), for a control whose action the option cannot name: a
	/// button style, a menu, a switch, a text field.
	static func click(at point: CGPoint) {
		guard let window = NSApp.keyWindow, let content = window.contentView else { return }
		let location = CGPoint(x: point.x, y: content.bounds.height - point.y)
		for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
			guard let event = NSEvent.mouseEvent(
				with: type,
				location: location,
				modifierFlags: [],
				timestamp: ProcessInfo.processInfo.systemUptime,
				windowNumber: window.windowNumber,
				context: nil,
				eventNumber: 0,
				clickCount: 1,
				pressure: type == .leftMouseDown ? 1 : 0,
			) else { continue }
			window.sendEvent(event)
		}
	}
}

/// Marks a control as an option of the shown step: placed for `h` `j` `k`
/// `l`, ringed while highlighted, pressed by ⏎.
private struct SetupKeyOption: ViewModifier {
	let activate: (() -> Void)?
	let cornerRadius: CGFloat
	@Environment(\.setupKeyboard) private var keyboard
	@Environment(\.registersSetupKeyOptions) private var registers
	@State private var id = UUID()

	func body(content: Content) -> some View {
		if let keyboard, registers {
			content
				.keyboardHighlight(keyboard.isHighlighted(id), cornerRadius: cornerRadius)
				.onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
					keyboard.place(id, at: frame)
					let center = CGPoint(x: frame.midX, y: frame.midY)
					SetupOptionActions.shared.set(id, activate ?? { SetupOptionActions.click(at: center) })
				}
				.onDisappear {
					keyboard.remove(id)
					SetupOptionActions.shared.remove(id)
				}
		} else {
			content
		}
	}
}

extension View {
	/// This control is an option of the setup's keys; ⏎ runs `activate`, or clicks the control.
	func setupKeyOption(cornerRadius: CGFloat = Tokens.Radius.radiusSm, activate: (() -> Void)? = nil) -> some View {
		modifier(SetupKeyOption(activate: activate, cornerRadius: cornerRadius))
	}
}
