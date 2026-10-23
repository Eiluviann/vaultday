import SwiftUI
import VaultdayCore

/// Step 7: security. The base version enforces no level: every workspace is
/// Off, the controls are drawn and say Planned, and nothing secret is shown or
/// stored here.
struct SecurityStep: View {
	let workspaces: WorkspacesModel
	let security: SecurityStepModel

	var body: some View {
		StepPane(title: "Security", lede: "App data is encrypted with a key held in the Secure Enclave") {
			SetupCallout(text: SecurityStepModel.plannedNote) { PlannedTag() }
			if let message = security.errorMessage {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: message)
			}
			HStack(alignment: .top, spacing: Tokens.Spacing.space12) {
				ForEach(SecurityLevel.allCases) { level in
					levelCard(level)
				}
			}
			protectionCard
			SetupCard(icon: "touchid", title: "Unlock Vaultday with Touch ID", description: "Your Mac password works as a fallback", tags: [SetupTag(text: "Planned")]) {
				SegmentedChoice(label: "Where unlocking applies", options: FeatureScopeSwitch.allCases.map { ($0, $0.title) }, selection: .constant(.off))
					.disabled(true)
			}
			SetupCard(icon: "key", title: "Recovery key", description: "Shown once. Without it, your encrypted app data is lost for good if this Mac is lost or reset", tags: [SetupTag(text: "Planned")]) {
				Button { } label: { SetupLabel(title: "Show") }
					.buttonStyle(SetupButtonStyle(isSmall: true))
					.disabled(true)
			}
			SetupCard(icon: "externaldrive", title: "Hardware key", description: "A security key unlocks Vaultday and can stand in for the recovery key", tags: [SetupTag(text: "Not set up"), SetupTag(text: "Planned")]) {
				Button { } label: { SetupLabel(title: "Add", icon: "plus") }
					.buttonStyle(SetupButtonStyle(isSmall: true))
					.disabled(true)
			}
		}
		.onAppear { security.refresh(workspaces: workspaces.entries) }
	}

	private func levelCard(_ level: SecurityLevel) -> some View {
		let icon = switch level {
		case .off: "lock.open"
		case .enhanced: "checkmark.shield"
		case .strict: "lock.shield"
		}
		return VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space6) {
				Image(systemName: icon).foregroundStyle(level == .enhanced ? Tokens.Colors.work : Tokens.Colors.inkMuted)
				Text(level.title).font(Tokens.Typography.headline.font).foregroundStyle(Tokens.Colors.ink)
				Spacer()
				if level != .off { PlannedTag() }
			}
			ForEach(level.points, id: \.self) { point in
				HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space6) {
					Text("•")
					Text(point)
				}
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
			}
		}
		.padding(Tokens.Spacing.space12)
		.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(level == .off ? Tokens.Colors.primary : Tokens.Colors.line, lineWidth: level == .off ? 2 : 1))
		.accessibilityElement(children: .combine)
		.accessibilityAddTraits(level == .off ? .isSelected : [])
	}

	private var protectionCard: some View {
		SetupCard(icon: "lock.shield", title: "Protection", description: "Which level applies where; the chips show the level each sphere or workspace gets", tags: [SetupTag(text: "Planned")]) {
			SegmentedChoice(label: "Where protection applies", options: FeatureScopeSwitch.allCases.map { ($0, $0.title) }, selection: .constant(.off))
				.disabled(true)
		} extra: {
			FlowRow(spacing: Tokens.Spacing.space6) {
				ForEach(Array(workspaces.entries.enumerated()), id: \.element.id) { index, entry in
					ScopeChip(title: "\(entry.name) · \(security.level(of: entry.id).title)", number: index + 1, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere), isOn: false) { }
						.disabled(true)
				}
			}
		}
	}
}
