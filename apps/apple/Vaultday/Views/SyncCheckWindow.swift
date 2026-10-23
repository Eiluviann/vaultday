import SwiftUI
import VaultdayCore

/// The sync check of the two-Mac test: this device, the document heads'
/// digest, the probes every device wrote. Opened from Window › Sync Check.
struct SyncCheckWindow: View {
	let app: AppModel
	@State private var isConfirmingRemoval = false

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			if let workspace = app.workspace, workspace.phase == .open {
				Text(workspace.name).font(Tokens.Typography.title.font)
				Text(workspace.appStateStatus).font(Tokens.Typography.callout.font).foregroundStyle(Tokens.Colors.inkSecondary)
				if let summary = workspace.syncSummary {
					LabeledContent("Workspace id", value: workspace.workspaceId)
					LabeledContent("This Mac", value: summary.deviceId)
					LabeledContent("Heads digest", value: summary.headsDigest)
					LabeledContent("Files not arrived", value: "\(summary.pendingFiles)")
					Text("PROBES").font(Tokens.Typography.eyebrow.font).foregroundStyle(Tokens.Colors.inkMuted)
					ForEach(summary.probes, id: \.self) { probe in
						Text(probe).font(Tokens.Typography.mono.font)
					}
				}
				HStack {
					Button("Write probe") { workspace.writeSyncProbe() }
					Button("Pull") { workspace.pull() }
					Spacer()
					Button("Remove workspace", role: .destructive) { isConfirmingRemoval = true }
				}
				.confirmationDialog("Remove \(workspace.name) from Vaultday?", isPresented: $isConfirmingRemoval) {
					Button("Remove workspace", role: .destructive) { app.removeWorkspace() }
				} message: {
					Text("Its key is deleted on every device, so its app state can no longer be read anywhere. The folder and its notes stay as they are.")
				}
			} else {
				Text("Open a workspace first").foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.font(Tokens.Typography.body.font)
		.padding(Tokens.Spacing.space24)
		.frame(minWidth: 460, minHeight: 320, alignment: .topLeading)
		.background(Tokens.Colors.surface)
	}
}
