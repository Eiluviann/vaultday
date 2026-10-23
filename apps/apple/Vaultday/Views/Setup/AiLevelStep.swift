import SwiftUI
import VaultdayCore

/// Step 9: the AI level. The base version has no AI, so the choice is stored
/// and inert and says Planned.
struct AiLevelStep: View {
	let workspaces: WorkspacesModel
	let level: AiLevelStepModel

	var body: some View {
		StepPane(title: "AI level", lede: "Choose how much AI does in each workspace. AI Glance looks at one item at a time, so light and on-device models can handle it; AI Insight also reads related tasks, notes and mail, so it needs a larger model") {
			SetupCallout(text: AiLevelStepModel.plannedNote) { PlannedTag() }
			SetupCallout(text: "What a cloud model may see follows the workspace's security level: Enhanced sends compacted mail and messages only, Strict keeps AI Insight to on-device and self-hosted models")
			if let message = level.errorMessage {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: message)
			}
			HStack(alignment: .top, spacing: Tokens.Spacing.space12) {
				summaryCard(icon: "minus", title: "Off", points: ["No AI features", "Search matches words only"], note: nil)
				summaryCard(icon: "sparkle", title: "AI Glance", points: ["Sorts tasks and the inbox into actions", "Suggestions from the item alone", "AI-assisted search"], note: "Fits light and on-device models")
				summaryCard(icon: "sparkles", title: "AI Insight", points: ["Links tasks and mail to related work", "Rich suggestions from notes and history", "Full AI search across everything"], note: "Needs a larger model")
			}
			SetupCard(icon: "sparkles", title: "Level", description: "Which level applies where", tags: [SetupTag(text: "Planned")]) {
				SegmentedChoice(label: "Where the level applies", options: FeatureScopeSwitch.allCases.map { ($0, $0.title) }, selection: Binding(get: { level.scope.mode }, set: { level.setMode($0) }))
			} extra: {
				levelRows
			}
			SetupCard(icon: "lock.shield", title: "Data retention", description: "What a cloud provider may keep of your requests; providers that can't promise it are paused", tags: [SetupTag(text: "Planned")]) {
				SegmentedChoice(label: "Data retention", options: DataRetentionSetting.allCases.map { ($0, $0.title) }, selection: Binding(get: { level.retention }, set: { level.setRetention($0) }))
			}
		}
		.onAppear {
			level.refresh(workspaces: workspaces.entries)
			level.reload()
		}
	}

	@ViewBuilder
	private var levelRows: some View {
		switch level.scope.mode {
		case .off:
			EmptyView()
		case .everywhere:
			levelRow(title: "Everything", target: AiScope.wholeScope)
		case .perSphere:
			ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
				levelRow(title: sphere == .work ? "Work" : "Life", target: sphere.rawValue)
			}
		case .perWorkspace:
			ForEach(workspaces.entries) { entry in
				levelRow(title: entry.name, target: entry.id)
			}
			if workspaces.entries.isEmpty {
				Text("Add a workspace to pick its level").textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
	}

	private func levelRow(title: String, target: String) -> some View {
		HStack {
			Text(title).textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
			Spacer()
			SegmentedChoice(
				label: "Level for \(title)",
				options: AiLevelChoice.allCases.map { ($0, $0.title) },
				selection: Binding(get: { level.level(of: target) }, set: { level.setLevel($0, target: target) }),
			)
		}
	}

	private func summaryCard(icon: String, title: String, points: [String], note: String?) -> some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space6) {
				Image(systemName: icon).foregroundStyle(Tokens.Colors.inkMuted)
				Text(title).textStyle(Tokens.Typography.headline).foregroundStyle(Tokens.Colors.ink)
			}
			ForEach(points, id: \.self) { point in
				HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space6) {
					Text("•")
					Text(point)
				}
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkSecondary)
			}
			if let note {
				Text(note).textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.padding(Tokens.Spacing.space12)
		.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line, lineWidth: 1))
		.accessibilityElement(children: .combine)
	}
}
