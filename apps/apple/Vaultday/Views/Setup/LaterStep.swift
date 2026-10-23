import SwiftUI
import VaultdayCore

/// Steps 7 to 12 arrive with the next part of onboarding; this keeps the rail whole.
struct LaterStep: View {
	let step: SetupStep

	var body: some View {
		StepPane(title: step.title, lede: "This step arrives in a later release; everything you chose so far is kept") {
			SetupCallout(icon: "clock", text: "Back returns to the steps you have done") { PlannedTag() }
		}
	}
}
