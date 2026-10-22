import SwiftUI
import VaultdayCore

@main
struct VaultdayApp: App {
	@State private var model = TodayModel()

	var body: some Scene {
		WindowGroup("Today") {
			TodayWindow(model: model)
				.frame(minWidth: 520, minHeight: 480)
		}
	}
}
