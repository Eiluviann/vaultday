import SwiftUI
import VaultdayCore

/// Step 4: where the working copy lives and which service carries it. The
/// mode and service are stored; the step says that `.vaultday/sync` has to
/// travel with the notes and offers the `.gitignore` line for vaults in git.
struct SyncStep: View {
	let sync: SyncStepModel

	var body: some View {
		StepPane(title: "Sync and backup", lede: "Your notes are plain files. Choose where the working copy lives and how your other devices get it; your sync service does the carrying") {
			SetupCallout(text: SyncStepModel.syncFolderNote)
			HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
				ChoiceCard(icon: "externaldrive", title: "Local", description: "Notes stay on this Mac only", isSelected: sync.home == .local) { sync.setHome(.local) }
				ChoiceCard(icon: "laptopcomputer", title: "Local + Cloud Sync", description: "Local folder, synced to the cloud", tag: "Recommended", isSelected: sync.home == .localAndCloud) { sync.setHome(.localAndCloud) }
				ChoiceCard(icon: "icloud", title: "Cloud", description: "Notes live in a cloud folder", isSelected: sync.home == .cloud) { sync.setHome(.cloud) }
			}
			if sync.showsLocalOnlyWarning {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: SyncStepModel.localOnlyWarning)
			} else {
				SetupCallout(text: sync.home == .cloud ? "All your notes move to the cloud folder; your sync service keeps them in sync" : "Your notes stay where they are; your sync service carries them to your other devices")
				cloudService
			}
			gitCard
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				Eyebrow(text: "Safety")
				SetupCard(icon: "clock", title: "History", description: "Every change kept for 90 days, outside your folders", tags: [SetupTag(text: "Planned")]) { EmptyView() }
				SetupCard(icon: "externaldrive", title: "Nightly backup", description: "Destination and schedule; encrypted before it leaves this Mac", tags: [SetupTag(text: "Planned")]) { EmptyView() }
				SetupCard(icon: "terminal", title: "Keep AI tools out", description: "Deny rules keep Claude Code and similar tools out unless you open a folder on purpose", tags: [SetupTag(text: "Planned")]) { EmptyView() }
			}
		}
	}

	private var cloudService: some View {
		SetupCard(
			icon: "icloud",
			title: "Cloud service",
			description: sync.home == .cloud ? "Where your notes live in the cloud" : "Where the copy for your other devices lives",
			tags: [sync.isServiceChosen ? SetupTag(text: "Chosen", tone: .on, icon: "checkmark") : SetupTag(text: "Not chosen")],
		) {
			Button {
				sync.confirmService()
			} label: {
				SetupLabel(title: "Choose", icon: "checkmark")
			}
			.buttonStyle(SetupButtonStyle(isSmall: true))
			.disabled(sync.isServiceChosen)
		} extra: {
			SegmentedChoice(
				label: "Cloud service",
				options: SyncServiceChoice.allCases.map { ($0, $0.title) },
				selection: Binding(get: { sync.service }, set: { sync.setService($0) }),
			)
		}
	}

	private var gitCard: some View {
		SetupCard(icon: "arrow.triangle.branch", title: "Vaults kept in git", description: SyncStepModel.gitNote) {
			Button {
				sync.copyGitignoreLine()
			} label: {
				SetupLabel(title: "Copy line", icon: "doc.on.doc")
			}
			.buttonStyle(SetupButtonStyle(isSmall: true))
		} extra: {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				Text(sync.gitignoreLine)
					.textStyle(Tokens.Typography.mono)
					.foregroundStyle(Tokens.Colors.ink)
					.textSelection(.enabled)
					.padding(Tokens.Spacing.space8)
					.frame(maxWidth: .infinity, alignment: .leading)
					.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
				if sync.gitWorkspaces.isEmpty {
					Text("None of your workspaces sits in a git repository")
						.textStyle(Tokens.Typography.caption)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
				ForEach(sync.gitWorkspaces) { target in
					HStack(spacing: Tokens.Spacing.space8) {
						Text(target.workspaceName).textStyle(Tokens.Typography.callout).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
						Text(target.ignoreFilePath.abbreviatedPath)
							.textStyle(Tokens.Typography.caption)
							.foregroundStyle(Tokens.Colors.inkMuted)
							.lineLimit(1)
							.truncationMode(.middle)
						Spacer(minLength: 0)
						if target.hasLine {
							SetupTag(text: "Added", tone: .on, icon: "checkmark")
						} else {
							Button("Add to .gitignore") { sync.appendGitignoreLine(forWorkspace: target.id) }
								.buttonStyle(SetupButtonStyle(isSmall: true))
						}
					}
				}
				if let message = sync.gitMessage {
					Text(message).textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkSecondary)
				}
			}
		}
	}
}
