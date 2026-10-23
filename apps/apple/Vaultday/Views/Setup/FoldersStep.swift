import SwiftUI
import VaultdayCore

/// Step 3: each workspace's folders. The first is the workspace's own; the
/// Add-folder popover grants further ones, read in place.
struct FoldersStep: View {
	let workspaces: WorkspacesModel
	@State private var addingTo: String?

	var body: some View {
		StepPane(title: "Your folders", lede: "Notes are the heart of Vaultday: markdown pages with the tasks, links and dates inside them. Each workspace reads its folders in place; nothing is copied or moved") {
			SetupCallout(text: "Folders may nest; the deepest one owns a file. Two work workspaces may not share files")
			if let message = workspaces.message {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: message)
			}
			if workspaces.entries.isEmpty {
				SetupCallout(icon: "folder.badge.plus", text: "Add a workspace first; its folder is the first one listed here")
			}
			LazyVGrid(columns: [GridItem(.adaptive(minimum: 280), spacing: Tokens.Spacing.space16, alignment: .top)], alignment: .leading, spacing: Tokens.Spacing.space16) {
				ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
					let inSphere = workspaces.entries(in: sphere)
					if !inSphere.isEmpty {
						SpherePanel(sphere: sphere) {
							EmptyView()
						} content: {
							ForEach(inSphere) { entry in workspaceCard(entry) }
						}
					}
				}
			}
		}
	}

	private func workspaceCard(_ entry: WorkspaceEntry) -> some View {
		let number = (workspaces.entries.firstIndex(of: entry) ?? 0) + 1
		return VStack(spacing: 0) {
			HStack(spacing: Tokens.Spacing.space8) {
				WorkspaceBadge(number: number, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere))
				Text(entry.name).font(Tokens.Typography.headline.font).foregroundStyle(Tokens.Colors.ink)
				SetupTag(text: "Active", tone: .on, icon: "checkmark")
				Spacer()
				Button {
					addingTo = entry.id
				} label: {
					SetupLabel(title: "Add folder", icon: "plus")
				}
				.buttonStyle(SetupButtonStyle(isSmall: true))
				.popover(isPresented: Binding(get: { addingTo == entry.id }, set: { if !$0 { addingTo = nil } }), arrowEdge: .bottom) {
					AddFolderPopover(
						title: "Add a folder to \(entry.name)",
						existing: "Any folder of markdown: a whole vault or any subfolder of one. Vaultday reads it in place; nothing is copied or moved",
						fresh: "Make a folder with New Folder in the next window; the workspace reads it from then on",
						footnote: "A subfolder of another workspace's folder is fine: the deepest folder owns its files",
						choose: { isNew in
							addingTo = nil
							Task { await addFolder(to: entry, isNew: isNew) }
						},
					)
				}
			}
			.frame(minHeight: Tokens.Size.sizeRow)
			.padding(Tokens.Spacing.space12)
			VStack(spacing: 0) {
				ForEach(entry.folders) { folder in
					folderRow(folder, in: entry)
				}
			}
			.padding(.horizontal, Tokens.Spacing.space12)
			.padding(.bottom, Tokens.Spacing.space8)
		}
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line, lineWidth: 1))
	}

	private func folderRow(_ folder: FolderEntry, in entry: WorkspaceEntry) -> some View {
		let path = folder.path.abbreviatedPath as NSString
		return HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
			Image(systemName: "folder").frame(width: 18).foregroundStyle(Tokens.Colors.inkMuted)
			VStack(alignment: .leading, spacing: 2) {
				(Text(path.deletingLastPathComponent + "/").foregroundStyle(Tokens.Colors.inkMuted) + Text(path.lastPathComponent).foregroundStyle(Tokens.Colors.ink).bold())
					.font(Tokens.Typography.mono.font)
					.lineLimit(1)
					.truncationMode(.middle)
				HStack(spacing: Tokens.Spacing.space8) {
					Text(folder.kindLabel).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
					if folder.isWorkspaceRoot {
						Label("New notes go here", systemImage: "square.and.pencil")
					} else {
						Label("Read-only", systemImage: "lock")
					}
				}
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.labelStyle(.titleAndIcon)
			}
			Spacer(minLength: 0)
			if !folder.isWorkspaceRoot {
				Menu {
					Button("Remove folder", role: .destructive) { workspaces.removeFolder(folder.id, from: entry.id) }
				} label: {
					Image(systemName: "ellipsis")
				}
				.menuStyle(.borderlessButton)
				.menuIndicator(.hidden)
				.frame(width: Tokens.Size.sizeControlSm)
				.accessibilityLabel("Folder options for \(folder.name)")
			}
		}
		.padding(.vertical, Tokens.Spacing.space8)
	}

	private func addFolder(to entry: WorkspaceEntry, isNew: Bool) async {
		let message = isNew ? "Make a new folder, then choose it" : "Choose a folder for \(entry.name)"
		guard let url = await FolderPicker.pick(message: message, prompt: isNew ? "Use this folder" : "Choose") else { return }
		workspaces.addFolder(url, to: entry.id)
	}
}
