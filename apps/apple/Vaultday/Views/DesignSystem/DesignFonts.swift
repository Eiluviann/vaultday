import CoreText
import Foundation

/// The design system's two faces, Geist and Geist Mono, ship in the app as
/// variable fonts (`Vaultday/Fonts`, SIL Open Font License). They are
/// registered for the process before the first view draws; until then, and
/// if registration fails, `Font.custom` falls back to the system face.
enum DesignFonts {
	/// The bundled font files, by file name without extension.
	static let fileNames = ["Geist-Variable", "GeistMono-Variable"]

	/// Registers the bundled faces once; later calls do nothing.
	@MainActor
	static func registerBundledFonts(in bundle: Bundle = .main) {
		guard !isRegistered else { return }
		isRegistered = true
		for name in fileNames {
			guard let url = bundle.url(forResource: name, withExtension: "ttf") else { continue }
			// A face another copy of the app registered first is fine: the error says so and the face is there.
			CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
		}
	}

	/// Whether a family can be drawn: registered, or installed on the Mac.
	static func isAvailable(family: String) -> Bool {
		let names = CTFontManagerCopyAvailableFontFamilyNames() as? [String] ?? []
		return names.contains(family)
	}

	@MainActor private static var isRegistered = false
}
