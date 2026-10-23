import SwiftUI
import VaultdayCore

/// The week close-out (mockup "Week close-out on Mac"): the four steps in a
/// row, the current step's work below, Back and Next at the foot. Every
/// value comes from the core; the screen only asks it to act.
struct CloseOutScreen: View {
	@Bindable var model: CloseOutModel
	@Environment(\.dismiss) private var dismiss

	var body: some View {
		VStack(spacing: 0) {
			header
			Divider().overlay(Tokens.Colors.line)
			stepBar
			Divider().overlay(Tokens.Colors.line)
			ScrollView {
				VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
					if let error = model.errorMessage {
						SetupCallout(tone: .caution, icon: "exclamationmark.triangle", text: error)
					}
					if let snapshot = model.snapshot {
						content(snapshot)
					}
				}
				.padding(Tokens.Spacing.space24)
				.frame(maxWidth: .infinity, alignment: .leading)
			}
			Divider().overlay(Tokens.Colors.line)
			footer
		}
		.frame(width: 860, height: 640)
		.background(Tokens.Colors.surface)
		.onExitCommand { dismiss() }
	}

	// MARK: Frame

	private var header: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Text(model.title)
				.textStyle(Tokens.Typography.title)
				.foregroundStyle(Tokens.Colors.ink)
				.accessibilityAddTraits(.isHeader)
			Spacer()
			if let deadline = model.snapshot?.deadline {
				Text("Due \(CloseOutText.deadlineTitle(deadline))")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.padding(.horizontal, Tokens.Spacing.space24)
		.padding(.vertical, Tokens.Spacing.space16)
	}

	private var stepBar: some View {
		HStack(spacing: Tokens.Spacing.space4) {
			ForEach(Array(model.steps.enumerated()), id: \.element.id) { index, step in
				Button {
					model.select(step.kind)
				} label: {
					HStack(spacing: Tokens.Spacing.space6) {
						stepMark(step, number: index + 1)
						Text(step.title)
							.font(Tokens.Typography.callout.font.weight(model.currentStep == step.kind ? .semibold : .medium))
							.lineLimit(1)
					}
					.foregroundStyle(model.currentStep == step.kind ? Tokens.Colors.ink : Tokens.Colors.inkSecondary)
					.padding(.horizontal, Tokens.Spacing.space8)
					.padding(.vertical, 6)
					.background(model.currentStep == step.kind ? Tokens.Colors.surfaceRaised : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
					.contentShape(Rectangle())
				}
				.buttonStyle(.plain)
				.accessibilityLabel("Step \(index + 1), \(step.title), \(statusText(step.status))")
				.accessibilityAddTraits(model.currentStep == step.kind ? .isSelected : [])
			}
			Spacer(minLength: 0)
		}
		.padding(.horizontal, Tokens.Spacing.space16)
		.padding(.vertical, Tokens.Spacing.space8)
		.background(Tokens.Colors.surfaceSidebar)
	}

	@ViewBuilder
	private func stepMark(_ step: CloseOutStep, number: Int) -> some View {
		switch step.status {
		case .done:
			Image(systemName: "checkmark.circle.fill").foregroundStyle(Tokens.Colors.positive)
		case .skipped:
			Image(systemName: "minus.circle").foregroundStyle(Tokens.Colors.inkMuted)
		case .open:
			Image(systemName: "\(number).circle").foregroundStyle(Tokens.Colors.inkSecondary)
		}
	}

	private var footer: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			if let missing = model.snapshot?.missing, !missing.isEmpty, !model.isFinished {
				Text("Still open: \(missing.count)")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			Spacer()
			Button("Back") { model.goBack() }
				.buttonStyle(SetupButtonStyle())
				.disabled(!model.canGoBack)
			if model.canGoNext {
				Button("Next") { model.goNext() }
					.buttonStyle(SetupButtonStyle(kind: .primary))
			} else {
				Button("Done") { dismiss() }
					.buttonStyle(SetupButtonStyle(kind: .primary))
			}
		}
		.padding(.horizontal, Tokens.Spacing.space24)
		.padding(.vertical, Tokens.Spacing.space12)
	}

	// MARK: Steps

	@ViewBuilder
	private func content(_ snapshot: CloseOutSnapshot) -> some View {
		switch model.currentStep {
		case .toggl: TogglStep(model: model, snapshot: snapshot)
		case .allocate: AllocationStep(model: model, snapshot: snapshot)
		case .review: ReviewStep(model: model, snapshot: snapshot)
		case .table: TableStep(model: model, snapshot: snapshot)
		case .close: CloseStep(model: model, snapshot: snapshot)
		case nil: EmptyView()
		}
	}

	private func statusText(_ status: CloseOutStepStatus) -> String {
		switch status {
		case .open: "open"
		case .done: "done"
		case .skipped: "skipped"
		}
	}
}

/// A step's title and its one-line explanation.
private struct StepHeading: View {
	let title: String
	let note: String

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text(title)
				.textStyle(Tokens.Typography.heading)
				.foregroundStyle(Tokens.Colors.ink)
			Text(note)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.fixedSize(horizontal: false, vertical: true)
		}
	}
}

// MARK: Step 1

/// The Toggl entries: the flat days of an FTE week or the week's sessions;
/// a dry run while the Toggl feature set is off.
private struct TogglStep: View {
	let model: CloseOutModel
	let snapshot: CloseOutSnapshot

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			if snapshot.billing == .fte {
				StepHeading(title: "Toggl flat entries", note: "One flat entry per working day, no task detail; leave and holidays get none")
				VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
					ForEach(snapshot.flat) { day in
						HStack {
							Text(day.title).foregroundStyle(Tokens.Colors.ink)
							Spacer()
							Text(day.stateText(isTogglOn: snapshot.isTogglOn))
								.fontWeight(.semibold)
								.foregroundStyle(day.state == .failed ? Tokens.Colors.danger : Tokens.Colors.inkSecondary)
						}
						.textStyle(Tokens.Typography.callout)
						.accessibilityElement(children: .combine)
					}
				}
				.padding(Tokens.Spacing.space12)
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
			} else {
				StepHeading(title: "Toggl entries", note: "Every tracked session of the week goes to the Toggl queue with the billing rule applied")
				Text(snapshot.unqueuedSessions == 0 ? "Every session is queued" : "\(snapshot.unqueuedSessions) sessions not queued yet")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.ink)
			}
			if snapshot.isTogglOn {
				Button(snapshot.billing == .fte ? "Queue flat entries" : "Queue sessions") { model.queueTogglEntries() }
					.buttonStyle(SetupButtonStyle(kind: .primary))
					.disabled(model.steps.first?.status != .open)
				Text("Entries wait in the Toggl queue; the queue sends them when live access is allowed")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
			} else {
				SetupCallout(tone: .neutral, icon: "eye", text: "Dry run: Toggl is off for this workspace, so nothing is queued or sent")
			}
		}
	}
}

// MARK: Step 2

/// The FTE allocation: per working day, the stories touched that day with
/// their minutes; a change re-balances the rest of the day to keep its total.
private struct AllocationStep: View {
	let model: CloseOutModel
	let snapshot: CloseOutSnapshot

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			StepHeading(
				title: "Allocate the logged hours",
				note: "Each working day spread over the stories you touched that day, weighted by remaining estimate; no story goes past its estimate. Adjust a story and the day re-balances.",
			)
			ForEach(snapshot.allocation) { day in
				VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
					HStack {
						Text(CloseOutText.dayTitle(day.date)).textStyle(Tokens.Typography.body).fontWeight(.semibold)
						if day.isConfirmed {
							Image(systemName: "checkmark.circle.fill").foregroundStyle(Tokens.Colors.positive).accessibilityLabel("Confirmed")
						}
						Spacer()
						Text("\(CloseOutText.hours(day.totalMinutes)) of \(CloseOutText.hours(day.targetMinutes)) h")
							.textStyle(Tokens.Typography.callout)
							.foregroundStyle(day.totalMinutes == day.targetMinutes ? Tokens.Colors.inkSecondary : Tokens.Colors.caution)
					}
					if day.rows.isEmpty {
						Text("No story touched")
							.textStyle(Tokens.Typography.callout)
							.foregroundStyle(Tokens.Colors.inkMuted)
					}
					ForEach(day.rows) { row in
						HStack(spacing: Tokens.Spacing.space8) {
							Text(row.story).textStyle(Tokens.Typography.mono).foregroundStyle(Tokens.Colors.ink)
							Text(row.title).textStyle(Tokens.Typography.callout).foregroundStyle(Tokens.Colors.inkSecondary).lineLimit(1)
							if !row.hasEstimate {
								Text("No estimate")
									.textStyle(Tokens.Typography.caption)
									.foregroundStyle(Tokens.Colors.caution)
							}
							Spacer()
							Text("tracked \(DurationText.minutes(row.trackedMinutes))")
								.textStyle(Tokens.Typography.caption)
								.foregroundStyle(Tokens.Colors.inkMuted)
							Stepper(
								"\(CloseOutText.hours(row.minutes)) h",
								onIncrement: { model.setMinutes(row.minutes + 15, story: row.story, day: day.date) },
								onDecrement: { model.setMinutes(max(0, row.minutes - 15), story: row.story, day: day.date) },
							)
							.textStyle(Tokens.Typography.callout).monospacedDigit()
							.fixedSize()
							.disabled(day.isConfirmed)
							.accessibilityLabel("\(row.story) on \(CloseOutText.dayTitle(day.date))")
						}
					}
					if !day.unestimatedStories.isEmpty {
						SetupCallout(
							tone: .caution,
							icon: "exclamationmark.triangle",
							text: "Without an estimate no time goes to \(day.unestimatedStories.joined(separator: ", ")); add one in the story note to allocate minutes",
						)
					}
				}
				.padding(Tokens.Spacing.space12)
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
			}
			Text("Tracked time is private and never leaves this Mac")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
			Button("Confirm allocation") { model.confirmAllocation() }
				.buttonStyle(SetupButtonStyle(kind: .primary))
				.disabled(snapshot.allocation.allSatisfy(\.isConfirmed))
		}
	}
}

/// Hourly and fixed price: the billed hours by work item, the cap and the
/// unpaid overrun.
private struct ReviewStep: View {
	let model: CloseOutModel
	let snapshot: CloseOutSnapshot

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			StepHeading(
				title: snapshot.billing == .hourly ? "Review billable hours" : "Review the caps",
				note: snapshot.billing == .hourly
					? "Each session rounded up to the workspace increment; tasks marked not billable stay apart"
					: "Billed time stops at each task's estimate; what goes past it is an unpaid overrun",
			)
			if snapshot.review.isEmpty {
				Text("No session this week").textStyle(Tokens.Typography.callout).foregroundStyle(Tokens.Colors.inkMuted)
			}
			ForEach(snapshot.review) { row in
				HStack(spacing: Tokens.Spacing.space8) {
					Text(row.title).foregroundStyle(Tokens.Colors.ink).lineLimit(1)
					Spacer()
					Text("tracked \(DurationText.minutes(row.trackedMinutes))").foregroundStyle(Tokens.Colors.inkMuted)
					Text("billed \(DurationText.minutes(row.billedMinutes))").fontWeight(.semibold)
					if snapshot.billing == .hourly, !row.isBillable {
						Text("Not billable").foregroundStyle(Tokens.Colors.inkMuted)
					}
					if row.overrunMinutes > 0 {
						Text("overrun \(DurationText.minutes(row.overrunMinutes)), unpaid").foregroundStyle(Tokens.Colors.danger)
					}
				}
				.textStyle(Tokens.Typography.callout)
				.accessibilityElement(children: .combine)
			}
			Button("Confirm review") { model.confirmReview() }
				.buttonStyle(SetupButtonStyle(kind: .primary))
				.disabled(model.steps.first { $0.kind == .review }?.status != .open)
		}
	}
}

// MARK: Step 3

/// The copyable table, plain TSV, each row ticked as entered.
private struct TableStep: View {
	let model: CloseOutModel
	let snapshot: CloseOutSnapshot

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			StepHeading(title: snapshot.table.title, note: "Copy the table and paste it into the time sheet; tick each row as you enter it. The Monday alert lists what is still open.")
			Grid(alignment: .leading, horizontalSpacing: Tokens.Spacing.space12, verticalSpacing: Tokens.Spacing.space6) {
				GridRow {
					Text("")
					ForEach(Array(snapshot.table.columns.enumerated()), id: \.offset) { _, column in
						Text(column)
							.textStyle(Tokens.Typography.caption).fontWeight(.semibold)
							.foregroundStyle(Tokens.Colors.inkMuted)
					}
				}
				ForEach(snapshot.table.rows) { row in
					GridRow {
						Toggle(isOn: Binding(get: { row.isTicked }, set: { _ in model.toggleTick(row) })) { EmptyView() }
							.toggleStyle(.checkbox)
							.labelsHidden()
							.accessibilityLabel("\(row.key) entered")
						ForEach(Array(row.cells.enumerated()), id: \.offset) { _, cell in
							Text(cell)
								.textStyle(Tokens.Typography.callout).monospacedDigit()
								.foregroundStyle(row.isTicked ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
								.lineLimit(1)
						}
					}
				}
			}
			.padding(Tokens.Spacing.space12)
			.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
			HStack(spacing: Tokens.Spacing.space8) {
				Button {
					model.copyTable()
				} label: {
					Label("Copy table", systemImage: "doc.on.doc")
				}
				.buttonStyle(SetupButtonStyle(kind: .primary))
				.disabled(snapshot.table.rows.isEmpty)
				if model.isTableCopied {
					Text("Copied as tab-separated text").textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
				}
			}
		}
	}
}

// MARK: Step 4

/// The closing line: written once every other step is done.
private struct CloseStep: View {
	let model: CloseOutModel
	let snapshot: CloseOutSnapshot

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			let noun = snapshot.kind == .week ? "week" : "month"
			StepHeading(title: "Close the \(noun)", note: "Appends status · \(noun) closed to the day log of the \(noun)'s last working day")
			if snapshot.isClosed {
				Label(model.closedLine ?? "The \(noun) is closed", systemImage: "checkmark.seal.fill")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.positive)
			} else {
				let open = snapshot.missing.filter { !$0.hasPrefix("The \(noun)") }
				if !open.isEmpty {
					VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
						ForEach(open, id: \.self) { line in
							Label(line, systemImage: "circle")
								.textStyle(Tokens.Typography.callout)
								.foregroundStyle(Tokens.Colors.inkSecondary)
						}
					}
				}
				Button("Close the \(noun)") { model.close() }
					.buttonStyle(SetupButtonStyle(kind: .primary))
					.disabled(!snapshot.canClose)
			}
		}
	}
}

/// The Monday 07:30 alert, in the window: what the week before still misses.
struct CloseOutAlertBanner: View {
	let alert: CloseOutAlert
	let open: () -> Void
	let dismiss: () -> Void

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space6) {
				Image(systemName: "bell.badge").foregroundStyle(Tokens.Colors.caution)
				Text(alert.title).textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
			}
			ForEach(alert.missing, id: \.self) { line in
				Text(line).textStyle(Tokens.Typography.callout).foregroundStyle(Tokens.Colors.inkSecondary)
			}
			HStack(spacing: Tokens.Spacing.space8) {
				Spacer()
				Button("Later", action: dismiss).buttonStyle(SetupButtonStyle(isSmall: true))
				Button("Open close-out", action: open).buttonStyle(SetupButtonStyle(kind: .primary, isSmall: true))
			}
		}
		.padding(Tokens.Spacing.space12)
		.frame(width: 320)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
		.shadow(color: .black.opacity(0.12), radius: 12, y: 4)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("\(alert.workspaceName), \(alert.title)")
	}
}
