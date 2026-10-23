import SwiftUI
import VaultdayCore

/// Step 2: the workspaces of each sphere. A workspace is one folder holding
/// `.vaultday/settings.json`, so Add creates one in an empty folder or
/// imports a folder that already holds it; nothing is written until the
/// Configure sheet's Done. Other folders join a workspace on the next step.
struct WorkspacesStep: View {
	let workspaces: WorkspacesModel
	/// Screen-share mode leaves the life sphere out.
	var showsSphere: (WorkspaceSphere) -> Bool = { _ in true }
	@State private var configuring: ConfigureTarget?
	@State private var drafting: WorkspaceDraft?
	/// The draft on screen, kept past the sheet's own binding, which is
	/// already cleared when its dismissal runs.
	@State private var shownDraft: WorkspaceDraft?
	@State private var removing: WorkspaceEntry?
	@State private var addingTo: WorkspaceSphere?
	@State private var refusal: RefusedPick?
	@State private var moveProblem: FolderProblem?
	@State private var targetedSphere: WorkspaceSphere?

	var body: some View {
		StepPane(title: "Your workspaces", lede: "Each sphere holds one or more workspaces. A workspace has its own folder, color and rules; other folders join it in the next step") {
			SetupCallout(text: "A workspace is one folder with a .vaultday folder inside. Create one in a new or empty folder, or import a folder that already holds one")
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
								.draggable(SetupDragItem.workspace(id: entry.id).text)
						}
					}
					.dropDestination(for: String.self) { texts, _ in
						drop(texts, on: sphere)
					} isTargeted: { isTargeted in
						if isTargeted {
							targetedSphere = sphere
						} else if targetedSphere == sphere {
							targetedSphere = nil
						}
					}
					.setupDropZone(isTargeted: targetedSphere == sphere)
				}
			}
		}
		.sheet(item: $configuring) { target in
			ConfigureSheet(model: target.model, onDone: { workspaces.reload() })
				.mainWindowChildSheet()
		}
		.sheet(item: $drafting, onDismiss: dropUnconfirmedDraft) { draft in
			// Done creates the workspace through the draft's model; Cancel leaves no trace.
			ConfigureSheet(model: draft.configure, onDone: {})
				.mainWindowChildSheet()
		}
		.alert(
			refusal?.problem.title ?? "",
			isPresented: Binding(get: { refusal != nil }, set: { if !$0 { refusal = nil } }),
			presenting: refusal,
		) { pick in
			Button("Choose another folder") { Task { await addWorkspace(sphere: pick.sphere, choice: pick.choice) } }
				.keyboardShortcut(.defaultAction)
			Button("Cancel", role: .cancel) {}
		} message: { pick in
			Text(pick.problem.message)
		}
		.alert(
			moveProblem?.title ?? "",
			isPresented: Binding(get: { moveProblem != nil }, set: { if !$0 { moveProblem = nil } }),
			presenting: moveProblem,
		) { _ in
			Button("OK", role: .cancel) {}
		} message: { problem in
			Text(problem.message)
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
				options: [
					AddFolderOption(icon: "plus", title: "Create a workspace", detail: "In a new folder, made with New Folder in the next window, or in an empty one") {
						choose(sphere: sphere, choice: .create)
					},
					AddFolderOption(icon: "folder", title: "Import a workspace", detail: "A folder that already holds a .vaultday folder keeps its settings") {
						choose(sphere: sphere, choice: .import)
					},
				],
				footnote: "Folders of notes join a workspace on the Folders step",
			)
		}
	}

	private func card(_ entry: WorkspaceEntry) -> some View {
		SetupCard(
			title: entry.name,
			tags: [entry.isActive ? SetupTag(text: "Active", tone: .on, icon: "checkmark") : SetupTag(text: "Inactive")],
		) {
			HStack(spacing: Tokens.Spacing.space6) {
				Button {
					configuring = ConfigureTarget(model: WorkspaceConfigureModel(root: entry.rootPath))
				} label: {
					SetupLabel(title: "Configure", icon: "gearshape")
				}
				.buttonStyle(SetupButtonStyle(isSmall: true))
				Menu {
					let otherSphere: WorkspaceSphere = entry.sphere == .work ? .life : .work
					Button(otherSphere == .work ? "Move to Work" : "Move to Life") { move(entry, to: otherSphere) }
					if entry.isActive {
						Button("Deactivate") { workspaces.setActive(false, workspaceId: entry.id) }
					} else {
						Button("Reactivate") { workspaces.setActive(true, workspaceId: entry.id) }
					}
					Divider()
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
			HStack(spacing: Tokens.Spacing.space8) {
				// Only active workspaces have a number: it is their ⌘ key in the sidebar.
				if let index = workspaces.activeEntries.firstIndex(of: entry) {
					WorkspaceBadge(number: index + 1, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere))
				}
				Text(entry.rootPath.abbreviatedPath)
					.font(Tokens.Typography.mono.font)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.lineLimit(1)
					.truncationMode(.middle)
			}
		}
	}

	/// A card dropped on a sphere: the workspace moves there.
	private func drop(_ texts: [String], on sphere: WorkspaceSphere) -> Bool {
		guard let text = texts.first, case let .workspace(id) = SetupDragItem(text: text), let entry = workspaces.entry(id: id) else { return false }
		move(entry, to: sphere)
		return true
	}

	private func move(_ entry: WorkspaceEntry, to sphere: WorkspaceSphere) {
		moveProblem = workspaces.moveWorkspace(entry.id, to: sphere)
	}

	private func choose(sphere: WorkspaceSphere, choice: AddWorkspaceChoice) {
		addingTo = nil
		Task { await addWorkspace(sphere: sphere, choice: choice) }
	}

	private func addWorkspace(sphere: WorkspaceSphere, choice: AddWorkspaceChoice) async {
		let message = choice == .create ? "Make a new folder or choose an empty one" : "Choose a folder that holds a .vaultday folder"
		guard let url = await FolderPicker.pick(message: message, prompt: choice == .create ? "Create here" : "Import", canCreate: choice == .create) else { return }
		switch workspaces.beginWorkspace(folder: url, sphere: sphere, choice: choice) {
		case let .draft(draft):
			shownDraft = draft
			drafting = draft
		case let .refused(problem):
			refusal = RefusedPick(problem: problem, sphere: sphere, choice: choice)
		case let .failed(reason):
			refusal = RefusedPick(problem: FolderProblem(title: "\(url.lastPathComponent) cannot be added", message: reason, workspaceName: ""), sphere: sphere, choice: choice)
		}
	}

	private func dropUnconfirmedDraft() {
		guard let draft = shownDraft else { return }
		workspaces.cancel(draft)
		shownDraft = nil
	}
}

/// A refused folder and what was being added, so the popup can offer the picker again.
private struct RefusedPick: Identifiable {
	var id: String { problem.id }
	let problem: FolderProblem
	let sphere: WorkspaceSphere
	let choice: AddWorkspaceChoice
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

/// One row of the Add popover.
struct AddFolderOption: Identifiable {
	var id: String { title }
	let icon: String
	let title: String
	let detail: String
	let choose: () -> Void
}

/// The Add popover of the Workspaces and Folders steps: one row per way to add.
struct AddFolderPopover: View {
	let title: String
	let options: [AddFolderOption]
	let footnote: String

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
			Text(title)
				.font(Tokens.Typography.heading.font)
				.foregroundStyle(Tokens.Colors.ink)
				.padding(.horizontal, Tokens.Spacing.space12)
				.padding(.top, 6)
				.padding(.bottom, Tokens.Spacing.space4)
			ForEach(options) { option in
				row(option)
			}
			Text(footnote)
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.padding(.horizontal, Tokens.Spacing.space12)
				.padding(.vertical, 6)
		}
		.padding(Tokens.Spacing.space8)
		.frame(width: 360)
	}

	private func row(_ option: AddFolderOption) -> some View {
		Button {
			option.choose()
		} label: {
			HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
				Image(systemName: option.icon).frame(width: 18).foregroundStyle(Tokens.Colors.primary)
				VStack(alignment: .leading, spacing: 2) {
					Text(option.title).font(Tokens.Typography.body.font.weight(.semibold)).foregroundStyle(Tokens.Colors.ink)
					Text(option.detail)
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
