import Foundation

/// A single stroke as Carbon's `RegisterEventHotKey` takes it: the virtual
/// key code of the US layout and the Carbon modifier mask. The values are
/// those of `Carbon.HIToolbox` (`kVK_ANSI_*`, `cmdKey`, `optionKey`…),
/// written out so the core package needs no Carbon import and the mapping
/// can be tested.
public struct HotKeyChord: Equatable, Sendable {
	public let keyCode: UInt32
	public let carbonModifiers: UInt32

	/// `nil` for a key the table has no code for, or a stroke without a
	/// modifier (a bare key would steal typing from every app).
	public init?(_ stroke: KeyStroke) {
		guard !stroke.modifiers.isEmpty, let keyCode = Self.keyCodes[stroke.key] else { return nil }
		var mask: UInt32 = 0
		if stroke.modifiers.contains(.command) { mask |= Self.cmdKey }
		if stroke.modifiers.contains(.shift) { mask |= Self.shiftKey }
		if stroke.modifiers.contains(.option) { mask |= Self.optionKey }
		if stroke.modifiers.contains(.control) { mask |= Self.controlKey }
		self.keyCode = keyCode
		carbonModifiers = mask
	}

	static let cmdKey: UInt32 = 0x0100
	static let shiftKey: UInt32 = 0x0200
	static let optionKey: UInt32 = 0x0800
	static let controlKey: UInt32 = 0x1000

	/// `kVK_ANSI_*` for letters and digits, and the named keys a hotkey may use.
	static let keyCodes: [String: UInt32] = [
		"a": 0x00, "s": 0x01, "d": 0x02, "f": 0x03, "h": 0x04, "g": 0x05, "z": 0x06, "x": 0x07,
		"c": 0x08, "v": 0x09, "b": 0x0B, "q": 0x0C, "w": 0x0D, "e": 0x0E, "r": 0x0F, "y": 0x10,
		"t": 0x11, "1": 0x12, "2": 0x13, "3": 0x14, "4": 0x15, "6": 0x16, "5": 0x17, "9": 0x19,
		"7": 0x1A, "8": 0x1C, "0": 0x1D, "o": 0x1F, "u": 0x20, "i": 0x22, "p": 0x23, "l": 0x25,
		"j": 0x26, "k": 0x28, "n": 0x2D, "m": 0x2E,
		"return": 0x24, "tab": 0x30, "space": 0x31, "delete": 0x33, "escape": 0x35,
		"left": 0x7B, "right": 0x7C, "down": 0x7D, "up": 0x7E,
	]
}
