import SwiftUI
import VaultdayCore

/// Step 6: the feature sets. None ships in the base version, so every tile
/// is tagged Planned; the four-way switch is stored through the core's
/// feature registry for when one does.
struct FeatureSetsStep: View {
	let workspaces: WorkspacesModel
	let features: FeatureSetsStepModel

	var body: some View {
		StepPane(title: "Feature sets", lede: "Notes, tasks, reminders, calendar, mail, timer and routines are always in; these connect outside tools") {
			SetupCallout(text: "A workspace can ask for a feature set; anything that may send data externally stays off until you switch it on") { PlannedTag() }
			ForEach(features.tiles) { tile in
				SetupCard(
					icon: icon(for: tile.id),
					title: tile.title,
					description: tile.description,
					tags: tags(for: tile),
				) {
					SegmentedChoice(
						label: "Where \(tile.title) is on",
						options: FeatureScopeSwitch.allCases.map { ($0, $0.title) },
						selection: Binding(get: { tile.scope }, set: { features.setScope($0, tile: tile.id) }),
					)
				} extra: {
					scopeRow(tile)
				}
			}
		}
		.onAppear { features.refresh(workspaces: workspaces.entries) }
	}

	@ViewBuilder
	private func scopeRow(_ tile: FeatureTile) -> some View {
		switch tile.scope {
		case .perSphere:
			HStack(spacing: Tokens.Spacing.space6) {
				ForEach(WorkspaceSphere.allCases, id: \.self) { sphere in
					ScopeChip(title: sphere == .work ? "Work" : "Life", sphere: sphere, isOn: tile.targets.contains(sphere.rawValue)) {
						features.setTarget(sphere.rawValue, isOn: !tile.targets.contains(sphere.rawValue), tile: tile.id)
					}
				}
			}
		case .perWorkspace:
			FlowRow(spacing: Tokens.Spacing.space6) {
				ForEach(Array(workspaces.entries.enumerated()), id: \.element.id) { index, entry in
					ScopeChip(title: entry.name, number: index + 1, color: WorkspaceColor.resolve(accent: entry.accent, sphere: entry.sphere), isOn: tile.targets.contains(entry.id)) {
						features.setTarget(entry.id, isOn: !tile.targets.contains(entry.id), tile: tile.id)
					}
				}
			}
			if workspaces.entries.isEmpty {
				Text("Add a workspace to pick it here").font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
			}
		case .off, .everywhere:
			EmptyView()
		}
	}

	private func tags(for tile: FeatureTile) -> [SetupTag] {
		var tags: [SetupTag] = []
		if tile.raisesEgress { tags.append(SetupTag(text: "May send data externally", tone: .caution, icon: "exclamationmark.triangle")) }
		tags.append(SetupTag(text: "Planned"))
		return tags
	}

	private func icon(for id: String) -> String {
		switch id {
		case "github", "azure-devops": "arrow.triangle.branch"
		case "servicenow": "tablecells"
		case "teams-work-mail": "tray.full"
		case "toggl-billing": "timer"
		default: "puzzlepiece"
		}
	}
}
