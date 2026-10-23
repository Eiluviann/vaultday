import Carbon.HIToolbox

/// A system-wide hotkey through Carbon's `RegisterEventHotKey`, which needs
/// no Accessibility permission and works in the sandbox: ⌥⌘T reaches the mini
/// tracker while another app is frontmost. One hotkey per app.
@MainActor
final class GlobalHotKey {
	@MainActor private static var action: (() -> Void)?
	private var hotKeyRef: EventHotKeyRef?
	private var handlerRef: EventHandlerRef?

	/// `nil` when the system refused it, such as another app holding the keys.
	init?(keyCode: UInt32, modifiers: UInt32, action: @escaping @MainActor () -> Void) {
		Self.action = action
		var eventType = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
		let installed = InstallEventHandler(GetApplicationEventTarget(), { _, _, _ in
			// Carbon delivers hotkey events on the main thread.
			MainActor.assumeIsolated { GlobalHotKey.action?() }
			return noErr
		}, 1, &eventType, nil, &handlerRef)
		guard installed == noErr else { return nil }
		// 'VDAY'
		let id = EventHotKeyID(signature: OSType(0x5644_4159), id: 1)
		guard RegisterEventHotKey(keyCode, modifiers, id, GetApplicationEventTarget(), 0, &hotKeyRef) == noErr else { return nil }
	}
}
