import AppKit
import Carbon.HIToolbox
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
	/// A field's chrome around it draws the mode line, below its own bottom-left corner.
	@Entry var drawsVimModeLineOutside = false
}

/// The Vim field inside a view, for the chrome around it to draw its mode line.
private struct VimFieldPreferenceKey: PreferenceKey {
	static let defaultValue: VimFieldController? = nil

	static func reduce(value: inout VimFieldController?, nextValue: () -> VimFieldController?) {
		value = value ?? nextValue()
	}
}

extension View {
	/// Draws the mode line of the Vim field inside this view under this
	/// view's bottom-left corner: for a field whose border and padding are
	/// drawn around it (quick add), so the line sits below the whole box.
	func vimModeLineBelow() -> some View {
		environment(\.drawsVimModeLineOutside, true)
			.overlayPreferenceValue(VimFieldPreferenceKey.self, alignment: .bottomLeading) { controller in
				if let controller { VimModeLine(controller: controller).hangingBelow() }
			}
	}
}

/// A one-line text input with Vim modes under the Vim motions scheme.
struct VimTextField: View {
	let placeholder: String
	@Binding var text: String
	/// Bumped to put the keys in this field (`i`, `/`, ⌘L).
	var focusRequest = 0
	/// Whether the field takes the keys when it appears (the change editor, a popover's first field).
	var focusesOnAppear = false
	/// Esc in Normal mode, before the field gives the keys up: `true` keeps
	/// the field focused (the quick-add confirmation closes first).
	var onEscape: (() -> Bool)?
	@State private var controller = VimFieldController(isMultiline: false)

	init(_ placeholder: String, text: Binding<String>, focusRequest: Int = 0, focusesOnAppear: Bool = false, onEscape: (() -> Bool)? = nil) {
		self.placeholder = placeholder
		_text = text
		self.focusRequest = focusRequest
		self.focusesOnAppear = focusesOnAppear
		self.onEscape = onEscape
	}

	var body: some View {
		TextField(placeholder, text: $text)
			.vimField(controller, focusRequest: focusRequest, focusesOnAppear: focusesOnAppear, onEscape: onEscape)
	}
}

/// The multi-line text input with Vim modes under the Vim motions scheme.
struct VimTextEditor: View {
	@Binding var text: String
	@State private var controller = VimFieldController(isMultiline: true)

	var body: some View {
		TextEditor(text: $text)
			.vimField(controller, focusRequest: 0, focusesOnAppear: false, onEscape: nil)
	}
}

/// Ties a field to its controller: the anchor that finds its AppKit view,
/// the mode line under its bottom-left corner, and, under Vim motions, the
/// router that reads its keys. Under Standard it is the plain field.
private struct VimFieldModifier: ViewModifier {
	let controller: VimFieldController
	let focusRequest: Int
	let focusesOnAppear: Bool
	let onEscape: (() -> Bool)?
	@Environment(\.keyScheme) private var scheme
	@Environment(\.drawsVimModeLineOutside) private var drawsModeLineOutside

	func body(content: Content) -> some View {
		content
			.background(VimFieldAnchor(controller: controller))
			// In the setup a field is one of the step's options: ⏎ clicks into it, in Normal mode.
			.setupKeyOption()
			.overlay(alignment: .bottomLeading) {
				if scheme == .vim, !drawsModeLineOutside { VimModeLine(controller: controller).hangingBelow() }
			}
			.preference(key: VimFieldPreferenceKey.self, value: scheme == .vim ? controller : nil)
			.onAppear {
				controller.onEscape = onEscape
				register(for: scheme)
				// The anchor joins the window a turn after the view appears.
				if focusesOnAppear { Task { @MainActor in controller.focusField() } }
			}
			.onChange(of: scheme) { _, scheme in register(for: scheme) }
			.onDisappear { VimFieldRouter.shared.unregister(controller) }
			// SwiftUI's focus state cannot move the keys from the Home window's
			// focusable root into a field inside it; AppKit's first responder can.
			.onChange(of: focusRequest) { controller.focusField() }
	}

	private func register(for scheme: KeyScheme) {
		if scheme == .vim {
			VimFieldRouter.shared.register(controller)
		} else {
			VimFieldRouter.shared.unregister(controller)
		}
	}
}

extension View {
	fileprivate func vimField(_ controller: VimFieldController, focusRequest: Int, focusesOnAppear: Bool, onEscape: (() -> Bool)?) -> some View {
		modifier(VimFieldModifier(controller: controller, focusRequest: focusRequest, focusesOnAppear: focusesOnAppear, onEscape: onEscape))
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
			.textStyle(Tokens.Typography.kbd)
			.fixedSize()
			.allowsHitTesting(false)
			.accessibilityLabel("Field in \(controller.mode.title.lowercased()) mode")
		}
	}
}

extension View {
	/// Hangs a bottom-leading overlay under its parent's bottom edge without
	/// taking room in the layout.
	fileprivate func hangingBelow() -> some View {
		frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
			.offset(y: 16)
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
	/// The field's AppKit view, found once and kept while it stays in its window.
	@ObservationIgnored private weak var editableView: NSView?
	/// The text view's selection as this controller last set it, to tell a click from its own write.
	@ObservationIgnored private var lastWrittenRange: NSRange?
	@ObservationIgnored private var originalSelectionAttributes: [NSAttributedString.Key: Any]?

	init(isMultiline: Bool) {
		self.isMultiline = isMultiline
		editor = VimEditor(isMultiline: isMultiline)
	}

	/// Puts the keys in the field: the AppKit text view or field inside the
	/// anchor's frame, found once and kept while it stays in the window.
	func focusField() {
		guard let anchor, let window = anchor.window, let root = window.contentView else { return }
		if editableView?.window !== window {
			editableView = Self.editableView(in: root, matching: anchor.convert(anchor.bounds, to: nil))
		}
		guard let editableView else { return }
		window.makeFirstResponder(editableView)
	}

	/// The editable text field or text view that fills `frame` (window
	/// coordinates) most closely: of those whose centre lies inside it, the
	/// one whose size differs least from it.
	private static func editableView(in root: NSView, matching frame: CGRect) -> NSView? {
		let area = frame.insetBy(dx: -2, dy: -2)
		var best: (view: NSView, difference: CGFloat)?
		var stack = [root]
		while let view = stack.popLast() {
			for subview in view.subviews where !subview.isHidden {
				let bounds = subview.convert(subview.bounds, to: nil)
				guard bounds.intersects(area) else { continue }
				let isEditable = (subview as? NSTextField)?.isEditable == true || (subview as? NSTextView)?.isEditable == true
				if isEditable, area.contains(CGPoint(x: bounds.midX, y: bounds.midY)) {
					let difference = abs(bounds.width - frame.width) + abs(bounds.height - frame.height)
					if difference < best?.difference ?? .infinity { best = (subview, difference) }
				}
				stack.append(subview)
			}
		}
		return best?.view
	}

	/// Whether `textView` is this field's: the field's own text view (a
	/// multi-line editor), or the field editor working for the field's
	/// own text field. Another field floating over this one (the palette)
	/// is never mistaken for it.
	func owns(_ textView: NSTextView) -> Bool {
		guard let anchor, let window = anchor.window, window === textView.window else { return false }
		if editableView?.window !== window, let root = window.contentView {
			editableView = Self.editableView(in: root, matching: anchor.convert(anchor.bounds, to: nil))
		}
		guard let editableView else { return false }
		return textView === editableView || (textView.delegate as? NSView) === editableView
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

	/// Puts a caret where Normal mode's block starts, before a ⌘ key acts on the text.
	func collapseBlock(in textView: NSTextView) {
		let range = textView.selectedRange()
		textView.setSelectedRange(NSRange(location: range.location, length: 0))
		lastWrittenRange = textView.selectedRange()
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
	/// Until when the next field to take the keys opens in Insert mode: a
	/// moment, so a request no field answered does not reach a later click.
	private var insertDeadline: TimeInterval = 0
	private var monitor: Any?
	private var observations: [ObjectIdentifier: NSKeyValueObservation] = [:]

	private struct WeakController {
		weak var controller: VimFieldController?
	}

	private static let escape: UInt16 = 53
	private static let returnKeys: Set<UInt16> = [36, 76]
	private static let backspace: UInt16 = 51
	private static let arrows: [UInt16: VimKey] = [123: .left, 124: .right, 125: .down, 126: .up]
	/// Tab, Home, End, Page Up, Page Down, forward delete.
	private static let appKitKeys: Set<UInt16> = [48, 115, 119, 116, 121, 117]

	/// The next field to take the keys opens in Insert mode (`i`, `o`, `/`).
	func startNextFieldInInsert() {
		insertDeadline = ProcessInfo.processInfo.systemUptime + 0.5
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
		controller.activate(textView, startsInInsert: ProcessInfo.processInfo.systemUptime < insertDeadline)
		insertDeadline = 0
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
		// An input method composing text owns Esc and ⏎ until it commits.
		if textView.hasMarkedText() { return false }
		// Tab, Home, End, the page keys, forward delete and the function keys keep AppKit's meaning.
		if Self.appKitKeys.contains(event.keyCode) || flags.contains(.function) && Self.arrows[event.keyCode] == nil { return false }
		if flags.contains(.command) {
			// ⌘V and the rest act at the caret, not on Normal mode's one-character block.
			if controller.editor.mode == .normal { controller.collapseBlock(in: textView) }
			return false
		}
		// A key with no character of its own (a dead key, `^` on a German or
		// Swedish layout) is a command in Normal and Visual mode: read as the
		// character its cap shows, and never handed to AppKit, whose composing
		// would type over the block cursor.
		guard let key = Self.key(of: event, flags: flags) ?? Self.deadKey(of: event, flags: flags) else {
			return controller.editor.mode != .insert
		}

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
		// ⌃[ is Esc, as in a terminal.
		if flags.contains(.control), event.characters == "\u{1B}" || event.charactersIgnoringModifiers == "[" { return .escape }
		if returnKeys.contains(event.keyCode) { return .enter }
		if event.keyCode == backspace { return .backspace }
		if let arrow = arrows[event.keyCode] { return arrow }
		if flags.contains(.control) {
			return event.charactersIgnoringModifiers?.lowercased().first.map { .control($0) }
		}
		return event.characters?.first.map { .character($0) }
	}

	/// A dead key as the character its cap shows, from the current layout.
	private static func deadKey(of event: NSEvent, flags: NSEvent.ModifierFlags) -> VimKey? {
		guard flags.isDisjoint(with: [.command, .control]),
		      let source = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
		      let property = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)
		else { return nil }
		let layout = Unmanaged<CFData>.fromOpaque(property).takeUnretainedValue()
		return VimText.spacingCharacter(keyCode: event.keyCode, modifierFlags: flags, layoutData: layout).map { .character($0) }
	}
}

/// Character offsets of `VimEditor` against the UTF-16 offsets of AppKit's text.
enum VimText {
	static func characterOffset(utf16 offset: Int, in text: String) -> Int {
		let utf16 = text.utf16
		let clamped = min(max(0, offset), utf16.count)
		guard var index = utf16.index(utf16.startIndex, offsetBy: clamped, limitedBy: utf16.endIndex) else { return text.count }
		// An offset inside a character (an emoji's second half) rounds down to its start.
		while index > utf16.startIndex, index.samePosition(in: text) == nil {
			index = utf16.index(before: index)
		}
		guard let characterIndex = index.samePosition(in: text) else { return 0 }
		return text.distance(from: text.startIndex, to: characterIndex)
	}

	/// The character `keyCode` types on `layoutData` (a `kTISPropertyUnicodeKeyLayoutData`)
	/// when it is pressed alone; a dead key types what it and Space type, `^`
	/// for the key that otherwise waits for a vowel.
	static func spacingCharacter(keyCode: UInt16, modifierFlags: NSEvent.ModifierFlags, layoutData: CFData) -> Character? {
		translate(keyCode: keyCode, modifierFlags: modifierFlags, layoutData: layoutData, completesDeadKey: true)
	}

	/// The character one press of `keyCode` types on `layoutData`; `nil` for
	/// a dead key, which types nothing until the next key.
	static func typedCharacter(keyCode: UInt16, modifierFlags: NSEvent.ModifierFlags, layoutData: CFData) -> Character? {
		translate(keyCode: keyCode, modifierFlags: modifierFlags, layoutData: layoutData, completesDeadKey: false)
	}

	private static func translate(keyCode: UInt16, modifierFlags: NSEvent.ModifierFlags, layoutData: CFData, completesDeadKey: Bool) -> Character? {
		guard let bytes = CFDataGetBytePtr(layoutData) else { return nil }
		// UCKeyTranslate takes the Carbon modifier bits shifted down by 8.
		var modifiers: UInt32 = 0
		if modifierFlags.contains(.shift) { modifiers |= UInt32(shiftKey >> 8) }
		if modifierFlags.contains(.option) { modifiers |= UInt32(optionKey >> 8) }
		if modifierFlags.contains(.capsLock) { modifiers |= UInt32(alphaLock >> 8) }
		return bytes.withMemoryRebound(to: UCKeyboardLayout.self, capacity: 1) { layout in
			var deadKeyState: UInt32 = 0
			if let typed = press(keyCode, modifiers: modifiers, on: layout, deadKeyState: &deadKeyState) { return typed }
			guard completesDeadKey, deadKeyState != 0 else { return nil }
			return press(UInt16(kVK_Space), modifiers: 0, on: layout, deadKeyState: &deadKeyState)
		}
	}

	private static func press(_ keyCode: UInt16, modifiers: UInt32, on layout: UnsafePointer<UCKeyboardLayout>, deadKeyState: inout UInt32) -> Character? {
		var length = 0
		var characters = [UniChar](repeating: 0, count: 4)
		let status = UCKeyTranslate(layout, keyCode, UInt16(kUCKeyActionDown), modifiers, UInt32(LMGetKbdType()), OptionBits(0), &deadKeyState, characters.count, &length, &characters)
		guard status == noErr, length > 0 else { return nil }
		return String(utf16CodeUnits: characters, count: length).first
	}

	static func utf16Range(_ range: Range<Int>, in text: String) -> NSRange {
		let lower = text.index(text.startIndex, offsetBy: min(range.lowerBound, text.count))
		let upper = text.index(lower, offsetBy: min(range.count, text.distance(from: lower, to: text.endIndex)))
		return NSRange(lower ..< upper, in: text)
	}
}
