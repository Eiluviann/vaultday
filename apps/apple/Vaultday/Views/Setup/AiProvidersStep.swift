import SwiftUI
import VaultdayCore

/// Step 10: AI providers. A key goes to the Keychain on this Mac and nowhere
/// else; the list shows a mask, never any part of a key. Inert in the base
/// version, so everything says Planned.
struct AiProvidersStep: View {
	let workspaces: WorkspacesModel
	let providers: AiProvidersStepModel
	@State private var adding: ProviderChoice?
	@State private var budgetText = ""

	var body: some View {
		StepPane(title: "AI providers", lede: "Connect the models Vaultday may use. Data retention from the previous step decides which providers can run; the rest stay paused") {
			SetupCallout(text: AiProvidersStepModel.plannedNote) { PlannedTag() }
			SetupCallout(text: "Add one or more keys per provider. A key can be the default for every workspace, or kept to a sphere or a workspace; a provider needs no default")
			if let message = providers.errorMessage {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: message)
			}
			Eyebrow(text: "Providers")
			ForEach(ProviderChoice.allCases) { provider in
				providerCard(provider)
				ForEach(providers.keys(of: provider)) { key in
					keyRow(key)
				}
			}
			Eyebrow(text: "Built in")
			SetupCard(icon: "apple.logo", title: "Apple on-device models", description: "Handles most AI Glance jobs; nothing leaves this Mac", tags: [SetupTag(text: "Planned")]) {
				SetupTag(text: "Always on", tone: .on)
			}
			Eyebrow(text: "Limits")
			SetupCard(icon: "dollarsign.circle", title: "Monthly cloud budget", description: "Cloud models pause when it is reached; on-device models keep working", tags: [SetupTag(text: "Planned")]) {
				VimTextField("No limit", text: $budgetText)
					.textFieldStyle(.roundedBorder)
					.frame(width: 100)
					.multilineTextAlignment(.trailing)
					.onSubmit { providers.setMonthlyBudget(budgetText) }
					.accessibilityLabel("Monthly cloud budget per month")
			} extra: {
				if let issue = providers.issues["budget"] {
					Text(issue).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.caution)
				}
			}
		}
		.onAppear {
			providers.reload()
			budgetText = providers.monthlyBudget.map(String.init) ?? ""
		}
		.sheet(item: $adding) { provider in
			AddKeySheet(provider: provider, workspaces: workspaces, providers: providers)
				.mainWindowChildSheet()
		}
	}

	private func providerCard(_ provider: ProviderChoice) -> some View {
		var tags: [SetupTag] = []
		if provider.isRecommended { tags.append(SetupTag(text: "Recommended", tone: .on)) }
		if provider.mayLeaveThisMac { tags.append(SetupTag(text: "May send data externally", tone: .caution, icon: "exclamationmark.triangle")) }
		if provider.mayLeaveThisMac, !provider.supportsZeroRetention { tags.append(SetupTag(text: "Does not support zero data retention", tone: .caution)) }
		tags.append(SetupTag(text: "Planned"))
		return SetupCard(icon: icon(for: provider), title: provider.title, description: provider.summary, tags: tags, footnote: providers.isPaused(provider) ? "Paused while zero data retention is chosen" : nil, footnoteTone: .caution) {
			Button {
				adding = provider
			} label: {
				SetupLabel(title: "Add", icon: "plus")
			}
			.buttonStyle(SetupButtonStyle(isSmall: true))
			.accessibilityLabel("Add to \(provider.title)")
		}
	}

	private func keyRow(_ key: ProviderKeyEntry) -> some View {
		SetupCard(icon: "key", title: key.name, description: description(of: key)) {
			HStack(spacing: Tokens.Spacing.space8) {
				if key.provider.needsKey {
					Text(ProviderKeyEntry.mask)
						.font(Tokens.Typography.mono.font)
						.foregroundStyle(Tokens.Colors.inkMuted)
						.accessibilityLabel(key.isKeyStored ? "Key kept in the Keychain" : "No key in the Keychain")
					if !key.isKeyStored {
						SetupTag(text: "Not in the Keychain", tone: .caution)
					}
				}
				Button(role: .destructive) {
					providers.remove(id: key.id)
				} label: {
					SetupLabel(title: "Remove")
				}
				.buttonStyle(SetupButtonStyle(isSmall: true))
				.accessibilityLabel("Remove \(key.name)")
			}
		}
		.padding(.leading, Tokens.Spacing.space24)
	}

	private func description(of key: ProviderKeyEntry) -> String {
		switch key.scope {
		case .default: "Default for every workspace without its own"
		case .perSphere: "Kept to " + key.targets.sorted().map { $0 == "work" ? "Work" : "Life" }.joined(separator: " and ")
		case .perWorkspace: "Kept to \(key.targets.count) \(key.targets.count == 1 ? "workspace" : "workspaces")"
		}
	}

	private func icon(for provider: ProviderChoice) -> String {
		switch provider {
		case .local: "server.rack"
		case .openRouter: "arrow.triangle.branch"
		case .claude: "sparkles"
		case .openAI: "cpu"
		case .gemini: "diamond"
		case .other: "network"
		}
	}
}

/// Board 10a: name, key, where it is used, models. No test request: the base
/// version makes no AI call, so nothing here claims the key works.
struct AddKeySheet: View {
	let provider: ProviderChoice
	let workspaces: WorkspacesModel
	let providers: AiProvidersStepModel
	@State private var draft: ProviderKeyDraft
	@Environment(\.dismiss) private var dismiss

	init(provider: ProviderChoice, workspaces: WorkspacesModel, providers: AiProvidersStepModel) {
		self.provider = provider
		self.workspaces = workspaces
		self.providers = providers
		_draft = State(initialValue: ProviderKeyDraft(provider: provider))
	}

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
			HStack(alignment: .top, spacing: Tokens.Spacing.space12) {
				Image(systemName: "sparkles").foregroundStyle(Tokens.Colors.primary).imageScale(.large)
				VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
					Text(provider.needsKey ? "Add a \(provider == .claude ? "Claude" : provider.title) key" : "Add \(provider.title)").font(Tokens.Typography.headline.font)
					Text(provider.needsKey ? "Vaultday talks to \(provider.title) with your own API key, and the provider bills the usage. A provider can hold several keys" : "Vaultday talks to a model that runs on this Mac or on a server you run")
						.font(Tokens.Typography.callout.font).foregroundStyle(Tokens.Colors.inkSecondary)
				}
			}
			field("Name", issue: providers.issues["name"]) {
				VimTextField("Work key", text: $draft.name).textFieldStyle(.roundedBorder)
			}
			if provider.needsKey {
				field("API key", issue: providers.issues["key"]) {
					SecureField("Paste the key", text: $draft.key).textFieldStyle(.roundedBorder).font(Tokens.Typography.mono.font)
				}
				Text(AiProvidersStepModel.keyFootnote).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
			}
			if provider.needsEndpoint {
				field("Address", issue: providers.issues["endpoint"]) {
					VimTextField("http://localhost:11434", text: $draft.endpoint).textFieldStyle(.roundedBorder).font(Tokens.Typography.mono.font)
				}
			}
			usedIn
			if let models = provider.models {
				HStack(alignment: .top, spacing: Tokens.Spacing.space12) {
					field("AI Insight") { modelMenu(models, label: "Model for AI Insight", selection: $draft.insightModel) }
					field("AI Glance, when too heavy for this Mac") { modelMenu(models, label: "Model for AI Glance", selection: $draft.glanceModel) }
				}
			}
			HStack(spacing: Tokens.Spacing.space8) {
				Spacer()
				Button { dismiss() } label: { SetupLabel(title: "Cancel") }.buttonStyle(SetupButtonStyle(kind: .plain))
				Button {
					if case .added = providers.add(draft) {
						draft.key = ""
						dismiss()
					}
				} label: { SetupLabel(title: "Add key") }
					.buttonStyle(SetupButtonStyle(kind: .primary))
					.keyboardShortcut(.defaultAction)
			}
		}
		.padding(Tokens.Spacing.space20)
		.frame(width: 560)
		.background(Tokens.Colors.surfaceRaised)
		.onDisappear { draft.key = "" }
	}

	private var usedIn: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			Text("Used in").font(Tokens.Typography.callout.font.weight(.semibold))
			SegmentedChoice(label: "Where this key is used", options: ProviderKeyScopeSetting.allCases.map { ($0, $0.title) }, selection: Binding(get: { draft.scope }, set: { scope in
				draft.scope = scope
				draft.targets = []
			}))
			switch draft.scope {
			case .default:
				EmptyView()
			case .perSphere:
				HStack(spacing: Tokens.Spacing.space6) {
					ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
						ScopeChip(title: sphere == .work ? "Work" : "Life", sphere: sphere, isOn: draft.targets.contains(sphere.rawValue)) { toggle(sphere.rawValue) }
					}
				}
			case .perWorkspace:
				FlowRow(spacing: Tokens.Spacing.space6) {
					ForEach(Array(workspaces.entries.enumerated()), id: \.element.id) { index, entry in
						ScopeChip(title: entry.name, number: index + 1, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere), isOn: draft.targets.contains(entry.id)) { toggle(entry.id) }
					}
				}
			}
			if let issue = providers.issues["scope"] {
				Text(issue).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.caution)
			}
			Text("Default makes it the key for every workspace without its own; a provider needs no default")
				.font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
		}
	}

	private func toggle(_ target: String) {
		if draft.targets.contains(target) { draft.targets.remove(target) } else { draft.targets.insert(target) }
	}

	private func modelMenu(_ models: [ProviderModel], label: String, selection: Binding<String?>) -> some View {
		SetupMenu(label: label, options: models.map { ($0.id, $0.title) }, selection: Binding(get: { selection.wrappedValue ?? models.first?.id ?? "" }, set: { selection.wrappedValue = $0 }))
	}

	private func field(_ title: String, issue: String? = nil, @ViewBuilder content: () -> some View) -> some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text(title).font(Tokens.Typography.callout.font.weight(.semibold))
			content()
			if let issue {
				Text(issue).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.caution)
			}
		}
		.frame(maxWidth: .infinity, alignment: .leading)
	}
}
