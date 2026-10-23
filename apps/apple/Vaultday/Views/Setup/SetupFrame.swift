import SwiftUI
import VaultdayCore

/// The setup flow below its title bar: the 240 px step rail, and beside it
/// the content pane holding the shown step above the sticky footer.
struct SetupFrame<Content: View>: View {
	let onboarding: OnboardingModel
	@Binding var scheme: KeyboardSchemeSetting
	let chooseRestoreFolder: (RestoreSource) -> Void
	let finish: () -> Void
	@ViewBuilder var content: Content

	var body: some View {
		HStack(spacing: 0) {
			SetupSidebar(onboarding: onboarding, scheme: $scheme)
			VStack(spacing: 0) {
				content
					.frame(maxWidth: .infinity, maxHeight: .infinity)
				SetupFooter(onboarding: onboarding, chooseRestoreFolder: chooseRestoreFolder, finish: finish)
					.padding(.horizontal, SetupLayout.gutter)
			}
			.frame(minWidth: SetupLayout.contentMinWidth, maxWidth: .infinity)
			.background(Tokens.Colors.surface)
		}
	}
}
