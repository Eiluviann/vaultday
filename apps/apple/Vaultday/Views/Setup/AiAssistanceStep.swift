import SwiftUI
import VaultdayCore

/// Step 11: AI assistance. Which features are on and where, what agents and
/// other AI apps may do, and the guardrails. Stored and inert in the base
/// version; the board's "what leaves this Mac" preview needs a cloud model, so
/// it is not drawn.
struct AiAssistanceStep: View {
	let workspaces: WorkspacesModel
	let assist: AiAssistanceStepModel

	var body: some View {
		StepPane(title: "AI assistance", lede: "Choose what AI helps with and where: everywhere, per sphere or per workspace. Features that work at both levels let you pick Glance or Insight; anything AI writes carries a sparkle until you change it") {
			SetupCallout(text: AiAssistanceStepModel.plannedNote) { PlannedTag() }
			SetupCallout(text: "AI suggests and you confirm, never sending, deleting or booking anything on its own; a workspace on AI Glance runs every feature at Glance and leaves Insight-only ones off")
			if let message = assist.errorMessage {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: message)
			}
			Eyebrow(text: "Features")
			ForEach(AiFeature.all) { feature in
				featureCard(feature)
			}
			Eyebrow(text: "Agents and tools")
			claudeCodeCard
			SetupCard(icon: "app.connected.to.app.below.fill", title: "Other AI apps", description: "Claude Desktop, Cursor and other MCP apps reach your tasks and notes through the vaultday command", tags: [SetupTag(text: "May send data externally", tone: .caution, icon: "exclamationmark.triangle"), SetupTag(text: "Planned")]) {
				SegmentedChoice(label: "What other AI apps may do", options: OtherAppsAccessSetting.allCases.map { ($0, $0.title) }, selection: Binding(get: { assist.otherApps }, set: { assist.setOtherApps($0) }))
			}
			SetupCard(icon: "hand.raised", title: "Changes by agents", description: "What an agent may change without asking you", tags: [SetupTag(text: "Planned")]) {
				SetupMenu(label: "Changes by agents", options: AgentChangesSetting.allCases.map { ($0, $0.title) }, selection: Binding(get: { assist.agentChanges }, set: { assist.setAgentChanges($0) }), width: 170)
			}
			Eyebrow(text: "Guardrails")
			SetupCard(icon: "eye", title: "Preview before sending", description: "Shows the compacted copy before a cloud model sees it", tags: [SetupTag(text: "Planned")]) {
				SetupSwitch(label: "Preview before sending", isOn: Binding(get: { assist.previewsBeforeSending }, set: { assist.setPreviewsBeforeSending($0) }))
			}
			SetupCard(icon: "list.bullet.rectangle", title: "AI activity log", description: "Every request, the model and what was sent, kept 30 days on this Mac", tags: [SetupTag(text: "Planned")]) {
				SetupSwitch(label: "AI activity log", isOn: Binding(get: { assist.keepsActivityLog }, set: { assist.setKeepsActivityLog($0) }))
			}
		}
		.onAppear {
			assist.refresh(workspaces: workspaces.entries)
			assist.reload()
		}
	}

	private func featureCard(_ feature: AiFeature) -> some View {
		let scope = assist.scope(of: feature.id)
		return SetupCard(icon: "sparkle", title: feature.title, description: feature.description, tags: tags(for: feature)) {
			HStack(spacing: Tokens.Spacing.space8) {
				if feature.tiers == .either {
					SegmentedChoice(label: "Level for \(feature.title)", options: AiTier.allCases.map { ($0, $0.title) }, selection: Binding(get: { scope.wholeTier }, set: { assist.setTier($0, feature: feature.id) }))
				}
				SegmentedChoice(label: "Where \(feature.title) is on", options: FeatureScopeSwitch.allCases.map { ($0, $0.title) }, selection: Binding(get: { scope.mode }, set: { assist.setMode($0, feature: feature.id) }))
			}
		} extra: {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
				scopeRow(scope: scope, toggle: { target, isOn in assist.setTarget(target, isOn: isOn, feature: feature.id) })
				if let note = feature.insightNote {
					Text(note).textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
				}
			}
		}
	}

	private var claudeCodeCard: some View {
		SetupCard(icon: "terminal", title: "Claude Code", description: "Open a workspace in Claude Code and run canned jobs from the palette", tags: [SetupTag(text: "May send data externally", tone: .caution, icon: "exclamationmark.triangle"), SetupTag(text: "Planned")]) {
			SegmentedChoice(label: "Where Claude Code may be used", options: FeatureScopeSwitch.allCases.map { ($0, $0.title) }, selection: Binding(get: { assist.claudeCode.mode }, set: { assist.setClaudeCodeMode($0) }))
		} extra: {
			scopeRow(scope: assist.claudeCode, toggle: { target, isOn in assist.setClaudeCodeTarget(target, isOn: isOn) })
		}
	}

	@ViewBuilder
	private func scopeRow(scope: AiScope, toggle: @escaping (String, Bool) -> Void) -> some View {
		switch scope.mode {
		case .perSphere:
			HStack(spacing: Tokens.Spacing.space6) {
				ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
					ScopeChip(title: sphere == .work ? "Work" : "Life", sphere: sphere, isOn: scope.targets.contains(sphere.rawValue)) {
						toggle(sphere.rawValue, !scope.targets.contains(sphere.rawValue))
					}
				}
			}
		case .perWorkspace:
			FlowRow(spacing: Tokens.Spacing.space6) {
				ForEach(Array(workspaces.entries.enumerated()), id: \.element.id) { index, entry in
					ScopeChip(title: entry.name, number: index + 1, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere), isOn: scope.targets.contains(entry.id)) {
						toggle(entry.id, !scope.targets.contains(entry.id))
					}
				}
			}
		case .off, .everywhere:
			EmptyView()
		}
	}

	private func tags(for feature: AiFeature) -> [SetupTag] {
		var tags: [SetupTag] = []
		switch feature.tiers {
		case .either: break
		case .glanceOnly: tags.append(SetupTag(text: "Glance"))
		case .insightOnly: tags.append(SetupTag(text: "Insight"))
		}
		tags.append(SetupTag(text: "Planned"))
		return tags
	}
}
