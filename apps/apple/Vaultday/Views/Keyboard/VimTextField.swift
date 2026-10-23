import AppKit
import SwiftUI
import VaultdayCore

// Vim inside text inputs. Under the Vim motions scheme every field the app
// draws with `VimTextField` (or `VimTextEditor`) has its own Normal, Insert
// and Visual modes, run by `VimEditor`; under Standard it is the plain
// SwiftUI field. The field stays a SwiftUI `TextField`, so it keeps its look
// and modifiers: one key monitor reads the keys before AppKit's field editor
// does, runs them through the field's editor, and writes the text and the
// cursor back (a one-character selection is Normal mode's block cursor).
// Insert mode's typing is AppKit's own, read back before the next key.

extension EnvironmentValues {
	/// The keyboard scheme the app runs; text inputs read it.
	@Entry var keyScheme: KeyScheme = .vim
}

/// A one-line text input with Vim modes under the Vim motions scheme.
struct VimTextField: View {
	let placeholder: String
	@Binding var text: String
	/// Esc in Normal mode, before the field gives the keys up: `true` keeps
	/// the field focused (the quick-add confirmation closes first).
	var onEscape: (() -> Bool)?
	@Environment(\.keyScheme) private var scheme
	@State private var controller = VimFieldController(isMultiline: false)

	init(_ placeholder: String, text: Binding<String>, onEscape: (() -> Bool)? = nil) {
		self.placeholder = placeholder
		_text = text
		self.onEscape = onEscape
	}

	var body: some View {
		if scheme == .vim {
			TextField(placeholder, text: $text)
				.vimField(controller, onEscape: onEscape)
		} else {
			TextField(placeholder, text: $text)
		}
	}
}

/// The multi-line text input with Vim modes under the Vim motions scheme.
struct VimTextEditor: View {
	@Binding var text: String
	@Environment(\.keyScheme) private var scheme
	@State private var controller = VimFieldController(isMultiline: true)

	var body: some View {
		if scheme == .vim {
			TextEditor(text: $text)
				.vimField(controller, onEscape: nil)
		} else {
			TextEditor(text: $text)
		}
	}
}

extension View {
	/// Ties the field to its Vim controller: the anchor that finds its text
	/// view, and the mode line under its bottom-left corner.
	fileprivate func vimField(_ controller: VimFieldController, onEscape: (() -> Bool)?) -> some View {
		background(VimFieldAnchor(controller: controller))
			.overlay(alignment: .bottomLeading) { VimModeLine(controller: controller) }
			.onAppear {
				controller.onEscape = onEscape
				VimFieldRouter.shared.register(controller)
			}
			.onDisappear { VimFieldRouter.shared.unregister(controller) }
	}
}

/// The field's own mode and pending keys, below its bottom-left corner,
/// so it is clear the field's Vim is listening and not the app's.
private struct VimModeLine: View {
	let controller: VimFieldController

	var body: some View {
		if controller.isActive {
			HStack(spacing: Tokens.Spacing.space6) {
				Text(controller.mode.title)
					.fontWeight(.semibold)
					.foregroundStyle(controller.mode == .normal ? Tokens.Colors.inkMuted : Tokens.Colors.focus)
				if !controller.pendingText.isEmpty {
					Text(controller.pendingText)
						.foregroundStyle(Tokens.Colors.ink)
				}
			}
			.font(.custom(Tokens.Typography.mono.family, size: 10))
			.fixedSize()
			.padding(.top, 3)
			// Hangs under the field without taking room in the layout.
			.alignmentGuide(.bottom) { $0[.top] }
			.allowsHitTesting(false)
			.accessibilityLabel("Field in \(controller.mode.title.lowercased()) mode")
		}
	}
}

/// An empty view behind the field: where it is in its window, so the
/// router can tell which field the window's text view belongs to.
private struct VimFieldAnchor: NSViewRepresentable {
	let controller: VimFieldController

	func makeNSView(context: Context) -> NSView {
		let view = NSView()
		controller.anchor = view
		return view
	}

	func updateNSView(_ view: NSView, context: Context) {
		controller.anchor = view
	}
}

/// One field's Vim state: its editor, and what the mode line shows.
@MainActor
@Observable
final class VimFieldController {
	let isMultiline: Bool
	private(set) var isActive = false
	private(set) var mode: VimMode = .normal
	private(set) var pendingText = ""
	@ObservationIgnored var editor: VimEditor
	@ObservationIgnored weak var anchor: NSView?
	@ObservationIgnored var onEscape: (() -> Bool)?
	/// The text view's selection as this controller last set it, to tell a click from its own write.
	@ObservationIgnored private var lastWrittenRange: NSRange?
	@ObservationIgnored private var originalSelectionAttributes: [NSAttributedString.Key: Any]?

	init(isMultiline: Bool) {
		self.isMultiline = isMultiline
		editor = VimEditor(isMultiline: isMultiline)
	}

	/// Whether `textView` is this field's: it lies inside the field's frame in the same window.
	func owns(_ textView: NSTextView) -> Bool {
		guard let anchor, anchor.window === textView.window, anchor.window != nil else { return false }
		let field = anchor.convert(anchor.bounds, to: nil).insetBy(dx: -2, dy: -2)
		let view = textView.convert(textView.visibleRect, to: nil)
		return field.contains(CGPoint(x: view.midX, y: view.midY))
	}

	/// The field took the keys: Normal mode, unless the key that opened it means insert.
	func activate(_ textView: NSTextView, startsInInsert: Bool) {
		let text = textView.string
		editor = VimEditor(text: text, isMultiline: isMultiline)
		if startsInInsert {
			editor.startInserting()
		} else {
			let selection = textView.selectedRange()
			// A caret from a click stays where it was put; a field that selected
			// all of itself on focus starts at its end.
			let caret = selection.length == 0 ? selection.location : (text as NSString).length
			editor.place(cursor: VimText.characterOffset(utf16: caret, in: text))
		}
		originalSelectionAttributes = textView.selectedTextAttributes
		isActive = true
		write(to: textView)
	}

	func deactivate(_ textView: NSTextView?) {
		if let textView, let originalSelectionAttributes {
			textView.selectedTextAttributes = originalSelectionAttributes
		}
		originalSelectionAttributes = nil
		lastWrittenRange = nil
		isActive = false
		pendingText = ""
	}

	/// Reads what AppKit changed since the last key: Insert mode's typing, a
	/// click that moved the caret, the binding setting new text.
	func read(from textView: NSTextView) {
		let text = textView.string
		let range = textView.selectedRange()
		if editor.mode == .insert {
			editor.syncInsert(text: text, cursor: VimText.characterOffset(utf16: range.location, in: text))
			return
		}
		editor.setText(text)
		if range != lastWrittenRange {
			editor.place(cursor: VimText.characterOffset(utf16: range.location, in: text))
		}
	}

	/// Writes the editor's text and cursor into the text view.
	func write(to textView: NSTextView) {
		let text = editor.text
		if textView.string != text {
			let whole = NSRange(location: 0, length: (textView.string as NSString).length)
			if textView.shouldChangeText(in: whole, replacementString: text) {
				textView.replaceCharacters(in: whole, with: text)
				textView.didChangeText()
			}
		}
		let range = VimText.utf16Range(editor.selection, in: text)
		textView.setSelectedRange(range)
		lastWrittenRange = range
		textView.selectedTextAttributes = selectionAttributes
		mode = editor.mode
		pendingText = editor.pendingText
	}

	/// Normal mode's block is an ink cell over the character; Visual
	/// mode's selection and Insert mode's caret are AppKit's own.
	private var selectionAttributes: [NSAttributedString.Key: Any] {
		guard editor.mode == .normal else { return originalSelectionAttributes ?? [:] }
		return [.backgroundColor: NSColor.textColor.withAlphaComponent(0.78), .foregroundColor: NSColor.textBackgroundColor]
	}
}

/// Routes keys to the Vim field whose text view holds the keys, before AppKit
/// types them; one per app.
@MainActor
final class VimFieldRouter {
	static let shared = VimFieldRouter()

	/// The yank register, shared by every field.
	private var register = VimRegister()
	private var controllers: [ObjectIdentifier: WeakController] = [:]
	private var active: (controller: VimFieldController, textView: NSTextView)?
	private var startsInInsert = false
	private var monitor: Any?
	private var observations: [ObjectIdentifier: NSKeyValueObservation] = [:]

	private struct WeakController {
		weak var controller: VimFieldController?
	}

	private static let escape: UInt16 = 53
	private static let returnKeys: Set<UInt16> = [36, 76]
	private static let backspace: UInt16 = 51
	private static let arrows: [UInt16: VimKey] = [123: .left, 124: .right, 125: .down, 126: .up]

	/// The next field to take the keys opens in Insert mode (`i`, `o`, `/`).
	func startNextFieldInInsert() {
		startsInInsert = true
	}

	func register(_ controller: VimFieldController) {
		controllers[ObjectIdentifier(controller)] = WeakController(controller: controller)
		installMonitor()
		observeWindows()
	}

	func unregister(_ controller: VimFieldController) {
		controllers[ObjectIdentifier(controller)] = nil
		if active?.controller === controller {
			controller.deactivate(active?.textView)
			active = nil
		}
	}

	private func installMonitor() {
		guard monitor == nil else { return }
		monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
			// Only a Bool crosses back out of the actor; the event stays on this thread.
			let isConsumed = MainActor.assumeIsolated { self?.route(event) ?? false }
			return isConsumed ? nil : event
		}
	}

	/// Watches each window's first responder, so a field turns its Vim on
	/// (and its mode line) the moment it takes the keys, not at its first key.
	private func observeWindows() {
		for window in NSApp.windows where observations[ObjectIdentifier(window)] == nil {
			observations[ObjectIdentifier(window)] = window.observe(\.firstResponder, options: [.new]) { [weak self] window, _ in
				MainActor.assumeIsolated { self?.firstResponderChanged(in: window) }
			}
		}
	}

	private func firstResponderChanged(in window: NSWindow) {
		let textView = window.firstResponder as? NSTextView
		if let active, active.textView === textView, active.controller.isActive { return }
		if let active {
			active.controller.deactivate(active.textView)
			self.active = nil
		}
		guard let textView, let controller = owner(of: textView) else { return }
		active = (controller, textView)
		controller.activate(textView, startsInInsert: startsInInsert)
		startsInInsert = false
	}

	private func owner(of textView: NSTextView) -> VimFieldController? {
		controllers.values.lazy.compactMap(\.controller).first { $0.owns(textView) }
	}

	/// One key: the field's editor takes it (`true`), or it goes on to AppKit
	/// (Insert mode's typing, every ⌘ key, a submit).
	private func route(_ event: NSEvent) -> Bool {
		guard let window = event.window, let textView = window.firstResponder as? NSTextView else { return false }
		if active?.textView !== textView || active?.controller.isActive != true { firstResponderChanged(in: window) }
		guard let (controller, _) = active, active?.textView === textView else { return false }
		let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
		if flags.contains(.command) { return false }
		guard let key = Self.key(of: event, flags: flags) else { return false }

		controller.read(from: textView)
		controller.editor.register = register
		let effect = controller.editor.handle(key)
		register = controller.editor.register
		switch effect {
		case .passThrough:
			return false
		case .submit:
			controller.write(to: textView)
			// AppKit's own ⏎ ends the line and runs the field's onSubmit.
			return false
		case .leave:
			if controller.onEscape?() != true, window.firstResponder === textView {
				window.makeFirstResponder(nil)
			}
			return true
		case .handled:
			controller.write(to: textView)
			return true
		}
	}

	private static func key(of event: NSEvent, flags: NSEvent.ModifierFlags) -> VimKey? {
		if event.keyCode == escape { return .escape }
		if returnKeys.contains(event.keyCode) { return .enter }
		if event.keyCode == backspace { return .backspace }
		if let arrow = arrows[event.keyCode] { return arrow }
		if flags.contains(.control) {
			return event.charactersIgnoringModifiers?.lowercased().first.map { .control($0) }
		}
		return event.characters?.first.map { .character($0) }
	}
}

/// Character offsets of `VimEditor` against the UTF-16 offsets of AppKit's text.
enum VimText {
	static func characterOffset(utf16 offset: Int, in text: String) -> Int {
		let utf16 = text.utf16
		let clamped = min(max(0, offset), utf16.count)
		guard let index = utf16.index(utf16.startIndex, offsetBy: clamped, limitedBy: utf16.endIndex),
		      let characterIndex = index.samePosition(in: text)
		else { return text.count }
		return text.distance(from: text.startIndex, to: characterIndex)
	}

	static func utf16Range(_ range: Range<Int>, in text: String) -> NSRange {
		let lower = text.index(text.startIndex, offsetBy: min(range.lowerBound, text.count))
		let upper = text.index(lower, offsetBy: min(range.count, text.distance(from: lower, to: text.endIndex)))
		return NSRange(lower ..< upper, in: text)
	}
}
