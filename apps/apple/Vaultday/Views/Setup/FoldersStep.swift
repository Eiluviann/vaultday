import SwiftUI
import VaultdayCore

/// Step 3: each workspace's folders. A workspace keeps its notes in its own
/// folder; a new folder is made inside it, and an existing one joins it as a
/// link (it stays where it is) or by moving into it, chosen per folder.
struct FoldersStep: View {
	let workspaces: WorkspacesModel
	@State private var addingTo: String?
	@State private var naming: WorkspaceEntry?
	@State private var newFolderName = ""
	@State private var joining: PendingFolder?
	@State private var refusal: RefusedFolder?

	var body: some View {
		StepPane(title: "Your folders", lede: "Notes are the heart of Vaultday: markdown pages with the tasks, links and dates inside them. Each workspace keeps its notes in its own folder") {
			SetupCallout(text: "Another folder joins a workspace as a link, and stays where it is, or by moving into the workspace's folder. A folder belongs to one workspace only")
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
		.alert(
			naming.map { "New folder in \($0.name)" } ?? "",
			isPresented: Binding(get: { naming != nil }, set: { if !$0 { naming = nil } }),
			presenting: naming,
		) { entry in
			TextField("Name", text: $newFolderName)
			Button("Create") { createFolder(in: entry) }
				.keyboardShortcut(.defaultAction)
			Button("Cancel", role: .cancel) {}
		}
		.confirmationDialog(
			joining.map { "Add \($0.name) to \($0.workspaceName)" } ?? "",
			isPresented: Binding(get: { joining != nil }, set: { if !$0 { dropUnjoinedFolder() } }),
			titleVisibility: .visible,
			presenting: joining,
		) { pending in
			Button("Link it") { join(pending, by: .link) }
				.keyboardShortcut(.defaultAction)
			Button("Move it into \(pending.workspaceName)") { join(pending, by: .move) }
			Button("Cancel", role: .cancel) { dropUnjoinedFolder() }
		} message: { _ in
			Text("A link leaves the folder where it is, on this Mac only. Moving puts it inside the workspace's folder, where every device sees it")
		}
		.alert(
			refusal?.problem.title ?? "",
			isPresented: Binding(get: { refusal != nil }, set: { if !$0 { refusal = nil } }),
			presenting: refusal,
		) { refused in
			if let entry = workspaces.entry(id: refused.workspaceId) {
				Button("Choose another folder") { Task { await addExistingFolder(to: entry) } }
					.keyboardShortcut(.defaultAction)
			}
			Button("Cancel", role: .cancel) {}
		} message: { refused in
			Text(refused.problem.message)
		}
	}

	private func workspaceCard(_ entry: WorkspaceEntry) -> some View {
		VStack(spacing: 0) {
			HStack(spacing: Tokens.Spacing.space8) {
				if let index = workspaces.activeEntries.firstIndex(of: entry) {
					WorkspaceBadge(number: index + 1, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere))
				}
				Text(entry.name).font(Tokens.Typography.headline.font).foregroundStyle(Tokens.Colors.ink)
				if entry.isActive {
					SetupTag(text: "Active", tone: .on, icon: "checkmark")
				} else {
					SetupTag(text: "Inactive")
				}
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
						options: [
							AddFolderOption(icon: "plus", title: "Create a new folder", detail: "An empty folder inside \(entry.name)'s folder, ready to write in") {
								addingTo = nil
								newFolderName = ""
								naming = entry
							},
							AddFolderOption(icon: "folder", title: "Add an existing folder", detail: "Any folder of markdown, linked where it is or moved inside") {
								addingTo = nil
								Task { await addExistingFolder(to: entry) }
							},
						],
						footnote: "A folder that belongs to another workspace cannot join this one",
					)
				}
			}
			.frame(minHeight: Tokens.Size.sizeRow)
			.padding(Tokens.Spacing.space12)
			VStack(spacing: 0) {
				ForEach(entry.folders) { folder in
					folderRow(folder, in: entry)
				}
				ForEach(entry.subfolders, id: \.self) { name in
					subfolderRow(name, in: entry)
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
					}
				}
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.labelStyle(.titleAndIcon)
			}
			Spacer(minLength: 0)
			if !folder.isWorkspaceRoot {
				Menu {
					if folder.role == .unlinked {
						Button("Link again") { workspaces.relinkFolder(folder.id, in: entry.id) }
						Button("Remove folder", role: .destructive) { workspaces.unlinkFolder(folder.id, from: entry.id) }
					} else {
						Button("Unlink folder", role: .destructive) { workspaces.unlinkFolder(folder.id, from: entry.id) }
					}
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

	/// A plain folder inside the workspace's folder: made or moved in here, or there from the start.
	private func subfolderRow(_ name: String, in entry: WorkspaceEntry) -> some View {
		HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
			Image(systemName: "folder").frame(width: 18).foregroundStyle(Tokens.Colors.inkMuted)
			VStack(alignment: .leading, spacing: 2) {
				Text(name)
					.font(Tokens.Typography.mono.font)
					.foregroundStyle(Tokens.Colors.ink)
					.lineLimit(1)
					.truncationMode(.middle)
				Text("Folder in \(entry.name)")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			Spacer(minLength: 0)
		}
		.padding(.vertical, Tokens.Spacing.space8)
	}

	private func addExistingFolder(to entry: WorkspaceEntry) async {
		guard let url = await FolderPicker.pick(message: "Choose a folder for \(entry.name)", prompt: "Choose") else { return }
		switch workspaces.offerFolder(url, to: entry.id) {
		case let .ready(pending):
			joining = pending
		case let .refused(problem):
			refusal = RefusedFolder(problem: problem, workspaceId: entry.id)
		case let .failed(reason):
			refusal = RefusedFolder(problem: FolderProblem(title: "\(url.lastPathComponent) cannot be added", message: reason, workspaceName: ""), workspaceId: entry.id)
		}
	}

	private func join(_ pending: PendingFolder, by join: FolderJoin) {
		joining = nil
		let isJoined = join == .link ? workspaces.linkFolder(pending) : workspaces.moveFolder(pending)
		if !isJoined, let message = workspaces.message {
			refusal = RefusedFolder(problem: FolderProblem(title: "\(pending.name) was not added", message: message, workspaceName: pending.workspaceName), workspaceId: pending.workspaceId)
		}
	}

	private func dropUnjoinedFolder() {
		guard let pending = joining else { return }
		workspaces.cancel(pending)
		joining = nil
	}

	private func createFolder(in entry: WorkspaceEntry) {
		guard !workspaces.createFolder(named: newFolderName, in: entry.id), let message = workspaces.message else { return }
		refusal = RefusedFolder(problem: FolderProblem(title: "The folder was not made", message: message, workspaceName: entry.name), workspaceId: entry.id)
	}
}

/// A refused folder and the workspace it was offered to, so the popup can offer the picker again.
private struct RefusedFolder: Identifiable {
	var id: String { problem.id }
	let problem: FolderProblem
	let workspaceId: String
}
