import SwiftUI
import VaultdayCore

/// Step 2: the workspaces of each sphere. A workspace is a folder holding
/// `.vaultday/settings.json`, so Add asks for its first folder; the folders
/// step adds further ones.
struct WorkspacesStep: View {
	let workspaces: WorkspacesModel
	/// Screen-share mode leaves the life sphere out.
	var showsSphere: (WorkspaceSphere) -> Bool = { _ in true }
	@State private var configuring: ConfigureTarget?
	@State private var removing: WorkspaceEntry?
	@State private var addingTo: WorkspaceSphere?

	var body: some View {
		StepPane(title: "Your workspaces", lede: "Each sphere holds one or more workspaces. A workspace has its own folder, color and rules; further folders come in the next step") {
			SetupCallout(text: "A workspace is a folder with a .vaultday folder inside; Vaultday makes it for a new folder and reads an existing one in place")
			if let message = workspaces.message {
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: message)
			}
			ForEach(workspaces.problems) { problem in
				SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: "\(problem.path.abbreviatedPath): \(problem.reason)")
			}
			LazyVGrid(columns: [GridItem(.adaptive(minimum: 280), spacing: Tokens.Spacing.space16, alignment: .top)], alignment: .leading, spacing: Tokens.Spacing.space16) {
				ForEach(WorkspaceSphere.allCases.filter(showsSphere), id: \.self) { sphere in
					SpherePanel(sphere: sphere) {
						addButton(sphere)
					} content: {
						let inSphere = workspaces.entries(in: sphere)
						if inSphere.isEmpty {
							Text(sphere == .work ? "No work workspace yet" : "No life workspace yet")
								.font(Tokens.Typography.callout.font)
								.foregroundStyle(Tokens.Colors.inkMuted)
								.padding(.vertical, Tokens.Spacing.space8)
						}
						ForEach(inSphere) { entry in
							card(entry)
						}
					}
				}
			}
		}
		.sheet(item: $configuring) { target in
			ConfigureSheet(model: target.model, onDone: { workspaces.reload() })
				.mainWindowChildSheet()
		}
		.confirmationDialog(
			removing.map { "Remove \($0.name)?" } ?? "",
			isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
			titleVisibility: .visible,
			presenting: removing,
		) { entry in
			Button("Remove workspace", role: .destructive) { workspaces.removeWorkspace(id: entry.id) }
		} message: { _ in
			Text("Its notes stay in the folder. Vaultday forgets the workspace and its app state on every device")
		}
	}

	private func addButton(_ sphere: WorkspaceSphere) -> some View {
		Button {
			addingTo = sphere
		} label: {
			SetupLabel(title: "Add", icon: "plus")
		}
		.buttonStyle(SetupButtonStyle(isSmall: true))
		.accessibilityLabel("Add a \(sphere.rawValue) workspace")
		.popover(isPresented: Binding(get: { addingTo == sphere }, set: { if !$0 { addingTo = nil } }), arrowEdge: .bottom) {
			AddFolderPopover(
				title: "Add a \(sphere.rawValue) workspace",
				existing: "Any folder of markdown: a whole vault or any subfolder of one. A folder that already is a workspace keeps its settings",
				fresh: "Make a folder with New Folder in the next window; Vaultday adds its .vaultday folder, ready to write in",
				footnote: "Nothing is copied or moved; Vaultday reads folders in place",
				choose: { isNew in
					addingTo = nil
					Task { await addWorkspace(sphere: sphere, isNew: isNew) }
				},
			)
		}
	}

	private func card(_ entry: WorkspaceEntry) -> some View {
		let number = (workspaces.entries.firstIndex(of: entry) ?? 0) + 1
		return SetupCard(
			title: entry.name,
			tags: [SetupTag(text: "Active", tone: .on, icon: "checkmark")],
		) {
			HStack(spacing: Tokens.Spacing.space6) {
				Button {
					configuring = ConfigureTarget(model: WorkspaceConfigureModel(root: entry.rootPath))
				} label: {
					SetupLabel(title: "Configure", icon: "gearshape")
				}
				.buttonStyle(SetupButtonStyle(isSmall: true))
				Menu {
					Button("Remove workspace…", role: .destructive) { removing = entry }
				} label: {
					Image(systemName: "ellipsis")
				}
				.menuStyle(.borderlessButton)
				.menuIndicator(.hidden)
				.frame(width: Tokens.Size.sizeControlSm)
				.accessibilityLabel("Options for \(entry.name)")
			}
		} extra: {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
				HStack(spacing: Tokens.Spacing.space8) {
					WorkspaceBadge(number: number, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere))
					Text(entry.rootPath.abbreviatedPath)
						.font(Tokens.Typography.mono.font)
						.foregroundStyle(Tokens.Colors.inkSecondary)
						.lineLimit(1)
						.truncationMode(.middle)
				}
				Text(entry.folders.count == 1 ? "1 folder" : "\(entry.folders.count) folders")
					.font(Tokens.Typography.callout.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
	}

	private func addWorkspace(sphere: WorkspaceSphere, isNew: Bool) async {
		let message = isNew ? "Make a new folder, then choose it" : "Choose the workspace's folder"
		guard let url = await FolderPicker.pick(message: message, prompt: isNew ? "Use this folder" : "Choose", canCreate: true) else { return }
		if let entry = workspaces.addWorkspace(folder: url, sphere: sphere) {
			configuring = ConfigureTarget(model: WorkspaceConfigureModel(root: entry.rootPath))
		}
	}
}

/// Makes the Configure sheet's model usable as a sheet item.
struct ConfigureTarget: Identifiable {
	let id = UUID()
	let model: WorkspaceConfigureModel
	/// The billing fields, while Toggl and billing is on for the workspace.
	var billing: BillingSettingsModel?
	/// The close-out keys.
	var closeOut: CloseOutSettingsModel?
	/// Workspaces whose `Leave.md` it may read.
	var leaveSources: [String] = []
}

/// The Add-folder popover: an existing folder or a new one.
struct AddFolderPopover: View {
	let title: String
	let existing: String
	let fresh: String
	let footnote: String
	let choose: (_ isNewFolder: Bool) -> Void

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
			Text(title)
				.font(Tokens.Typography.heading.font)
				.foregroundStyle(Tokens.Colors.ink)
				.padding(.horizontal, Tokens.Spacing.space12)
				.padding(.top, 6)
				.padding(.bottom, Tokens.Spacing.space4)
			row(icon: "folder", title: "Choose an existing folder", detail: existing, isNew: false)
			row(icon: "plus", title: "Create a new folder", detail: fresh, isNew: true)
			Text(footnote)
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.padding(.horizontal, Tokens.Spacing.space12)
				.padding(.vertical, 6)
		}
		.padding(Tokens.Spacing.space8)
		.frame(width: 360)
	}

	private func row(icon: String, title: String, detail: String, isNew: Bool) -> some View {
		Button {
			choose(isNew)
		} label: {
			HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
				Image(systemName: icon).frame(width: 18).foregroundStyle(Tokens.Colors.primary)
				VStack(alignment: .leading, spacing: 2) {
					Text(title).font(Tokens.Typography.body.font.weight(.semibold)).foregroundStyle(Tokens.Colors.ink)
					Text(detail)
						.font(Tokens.Typography.caption.font)
						.foregroundStyle(Tokens.Colors.inkMuted)
						.fixedSize(horizontal: false, vertical: true)
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
