import SwiftUI
import VaultdayCore

/// The sticky footer: Back (Restore on the first step), the count, Continue.
struct SetupFooter: View {
	let onboarding: OnboardingModel
	let chooseRestoreFolder: (RestoreSource) -> Void
	@State private var isShowingRestore = false

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			if onboarding.isRestoreAvailable {
				Button {
					isShowingRestore.toggle()
				} label: {
					SetupLabel(title: "Restore", icon: "clock.arrow.circlepath")
				}
				.buttonStyle(SetupButtonStyle())
				.popover(isPresented: $isShowingRestore, arrowEdge: .top) { restorePopover }
			} else {
				Button {
					onboarding.goBack()
				} label: {
					SetupLabel(title: "Back", icon: "arrow.left")
				}
				.buttonStyle(SetupButtonStyle())
				.disabled(!onboarding.canGoBack)
			}
			Spacer()
			Text(onboarding.progressText)
				.font(Tokens.Typography.callout.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
			Button {
				onboarding.goForward()
			} label: {
				SetupLabel(title: onboarding.continueTitle, iconAfter: "arrow.right")
			}
			.buttonStyle(SetupButtonStyle(kind: .primary))
			.keyboardShortcut(.defaultAction)
			.disabled(!onboarding.canContinue)
		}
		.padding(.vertical, Tokens.Spacing.space16)
		.overlay(alignment: .top) { Rectangle().fill(Tokens.Colors.line).frame(height: 1) }
	}

	private var restorePopover: some View {
		VStack(alignment: .leading, spacing: 2) {
			Eyebrow(text: "Restore from").padding(.horizontal, Tokens.Spacing.space12).padding(.top, 6).padding(.bottom, Tokens.Spacing.space4)
			restoreRow(.iCloud, icon: "icloud", title: "iCloud Drive", detail: "Workspaces kept in a folder in iCloud Drive")
			restoreRow(.localFolder, icon: "folder", title: "A local folder", detail: "A backup, or a copy of your notes from another Mac")
			if let message = onboarding.restoreMessage {
				Text(message)
					.font(Tokens.Typography.caption.font.weight(.semibold))
					.foregroundStyle(Tokens.Colors.ink)
					.padding(.horizontal, Tokens.Spacing.space12)
					.padding(.top, 6)
			}
			Text("Protected app data never syncs; it stays on the Mac it was created on")
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.padding(.horizontal, Tokens.Spacing.space12)
				.padding(.top, 6)
				.padding(.bottom, Tokens.Spacing.space4)
		}
		.padding(Tokens.Spacing.space8)
		.frame(width: SetupLayout.popoverWidth)
	}

	private func restoreRow(_ source: RestoreSource, icon: String, title: String, detail: String) -> some View {
		Button {
			isShowingRestore = false
			chooseRestoreFolder(source)
		} label: {
			HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
				Image(systemName: icon).frame(width: 18).foregroundStyle(Tokens.Colors.primary)
				VStack(alignment: .leading, spacing: 0) {
					Text(title).font(Tokens.Typography.body.font.weight(.semibold)).foregroundStyle(Tokens.Colors.ink)
					Text(detail).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
				}
				Spacer(minLength: 0)
			}
			.padding(.horizontal, Tokens.Spacing.space12)
			.padding(.vertical, Tokens.Spacing.space8)
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
	}
}
