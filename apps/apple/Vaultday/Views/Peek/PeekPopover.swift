import AppKit
import SwiftUI
import VaultdayCore

extension View {
	/// Shows the peek popover from this view while the peek model holds `subject`,
	/// and registers the view's frame so a force click on it looks `subject` up.
	func peekPopover(app: AppModel, subject: PeekSubject?) -> some View {
		modifier(PeekPopoverModifier(app: app, subject: subject))
	}
}

private struct PeekPopoverModifier: ViewModifier {
	let app: AppModel
	let subject: PeekSubject?

	func body(content: Content) -> some View {
		content
			.popover(isPresented: isShowing, arrowEdge: .bottom) {
				if let peeked = app.peek.content {
					PeekPopover(app: app, content: peeked)
				}
			}
			.onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame in
				if let subject { app.peekTargets.frames[subject] = frame }
			}
			.onDisappear {
				if let subject { app.peekTargets.frames[subject] = nil }
			}
	}

	private var isShowing: Binding<Bool> {
		Binding(
			get: { subject != nil && app.peek.subject == subject },
			set: { isShown in
				if !isShown, app.peek.subject == subject { app.peek.dismiss() }
			},
		)
	}
}

/// The popover of the Peek board: the record's fields, the thread or note
/// excerpt, the last log lines, and the actions the row offers. Read-only.
struct PeekPopover: View {
	let app: AppModel
	let content: PeekContent
	@Environment(\.workspaceAccent) private var accent
	/// The popover is its own window, so it takes the keys while shown and
	/// hands them to the keymap's dispatcher, as the Home window does.
	@FocusState private var hasKeyFocus: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: 10) {
			HStack(spacing: Tokens.Spacing.space8) {
				ForEach(content.chips, id: \.self) { chip in
					MonoChip(text: chip, isRounded: false)
				}
				if let subtitle = content.subtitle {
					Text(subtitle)
						.textStyle(Tokens.Typography.callout)
						.foregroundStyle(Tokens.Colors.inkMuted)
						.lineLimit(1)
				}
				Spacer()
				if let badge = content.badge {
					Text(badge)
						.textStyle(Tokens.Typography.callout).fontWeight(.semibold)
						.foregroundStyle(Tokens.Colors.caution)
						.padding(.horizontal, 8)
						.padding(.vertical, 2)
						.background(Tokens.Colors.cautionSoft, in: Capsule())
				}
			}
			Text(content.title)
				.textStyle(Tokens.Typography.title)
				.foregroundStyle(Tokens.Colors.ink)
				.fixedSize(horizontal: false, vertical: true)
			if !content.fields.isEmpty {
				LazyVGrid(columns: [GridItem(.flexible(), spacing: 18), GridItem(.flexible())], alignment: .leading, spacing: 6) {
					ForEach(content.fields, id: \.label) { field in
						HStack(alignment: .firstTextBaseline) {
							Text(field.label).foregroundStyle(Tokens.Colors.inkMuted)
							Spacer(minLength: 10)
							Text(field.value).foregroundStyle(Tokens.Colors.ink).multilineTextAlignment(.trailing)
						}
						.textStyle(Tokens.Typography.callout)
					}
				}
			}
			if let excerpt = content.excerpt {
				Text(PreviewText.attributed(excerpt))
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkSecondary)
					.lineLimit(4)
					.padding(.horizontal, 10)
					.padding(.vertical, 8)
					.frame(maxWidth: .infinity, alignment: .leading)
					.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
			}
			if !content.activity.isEmpty {
				VStack(alignment: .leading, spacing: 3) {
					ForEach(content.activity) { entry in
						ActivityLine(entry: entry)
					}
				}
			}
			actions
		}
		.padding(.horizontal, 16)
		.padding(.vertical, 14)
		.frame(width: 440)
		.background(Tokens.Colors.surfaceRaised)
		.environment(\.workspaceAccent, accent)
		.focusable()
		.focusEffectDisabled()
		.focused($hasKeyFocus)
		.onAppear { hasKeyFocus = true }
		.onKeyPress(phases: .down) { press in
			app.handlePeekKey(KeyStroke(press)) ? .handled : .ignored
		}
	}

	private var actions: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			if let taskId = content.taskId {
				PeekButton(title: "Open", cap: app.keyLabel(.open), isPrimary: true, tint: accent) {
					app.openPreview(taskId: taskId)
				}
				PeekButton(title: "Start timer", cap: app.keyLabel(.toggleTimer)) {
					app.peek.dismiss()
					app.startTimer(taskId: taskId)
				}
				PeekButton(title: "Copy id", cap: nil) {
					NSPasteboard.general.clearContents()
					NSPasteboard.general.setString(taskId, forType: .string)
				}
			}
			if let number = content.storyNumber {
				PeekButton(title: "Full story", cap: nil) {
					app.peek.dismiss()
					app.handleLink(app.preview.fullStoryLink(storyNumber: number))
				}
			}
			Spacer()
			if let link = content.obsidianLink {
				Button {
					NSWorkspace.shared.open(link)
				} label: {
					Label("Obsidian", systemImage: "diamond")
						.labelStyle(TightLabelStyle())
						.foregroundStyle(Tokens.Colors.ink)
						.padding(.horizontal, 10)
						.frame(height: 28)
						.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
						.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(Tokens.Colors.lineButton))
				}
				.buttonStyle(.plain)
				.textStyle(Tokens.Typography.body)
			}
		}
		.padding(.top, 10)
		.overlay(alignment: .top) { Rectangle().fill(Tokens.Colors.lineFaint).frame(height: 1) }
	}
}

private struct PeekButton: View {
	let title: String
	let cap: String?
	var isPrimary = false
	var tint: Color = Tokens.Colors.primary
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			HStack(spacing: 5) {
				Text(title).fontWeight(isPrimary ? .semibold : .regular)
				if let cap {
					Text(cap)
						.textStyle(Tokens.Typography.kbd)
						.padding(.horizontal, 5)
						.padding(.vertical, 3)
						.background(isPrimary ? Color.white.opacity(0.25) : Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
				}
			}
			.padding(.horizontal, 10)
			.frame(height: 28)
			.foregroundStyle(isPrimary ? Tokens.Colors.onWs : Tokens.Colors.ink)
			.background(isPrimary ? tint : Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
			.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(isPrimary ? .clear : Tokens.Colors.lineButton))
		}
		.buttonStyle(.plain)
		.textStyle(Tokens.Typography.body)
	}
}

/// One line of the activity list: when, the kind, the text, and the model that wrote it.
struct ActivityLine: View {
	let entry: PreviewActivity

	var body: some View {
		(Text(entry.whenLabel).foregroundStyle(Tokens.Colors.inkMuted)
			+ Text(" · ")
			+ Text(entry.kind.map { "\($0) · " } ?? "")
			+ Text(entry.text)
			+ (entry.agent.map { Text(" · \($0)").foregroundStyle(Tokens.Colors.ai) } ?? Text("")))
			.textStyle(Tokens.Typography.callout)
			.foregroundStyle(entry.kind == "blocker" ? Tokens.Colors.caution : Tokens.Colors.inkSecondary)
			.lineLimit(2)
			.frame(maxWidth: .infinity, alignment: .leading)
	}
}

// MARK: Force click

/// Where each peekable view sits in the window, so a force click finds it.
/// Not observed: the frames change with every scroll.
@MainActor
final class PeekTargetRegistry {
	var frames: [PeekSubject: CGRect] = [:]

	/// The smallest registered frame holding `point` (in the window's top-left coordinates).
	func subject(at point: CGPoint) -> PeekSubject? {
		frames
			.filter { $0.value.contains(point) }
			.min { $0.value.width * $0.value.height < $1.value.width * $1.value.height }?
			.key
	}
}

/// Listens for the trackpad's second pressure stage (the force click) and
/// peeks the item under the pointer.
@MainActor
final class ForceClickMonitor {
	private var monitor: Any?
	private var wasForceClicking = false

	func start(app: AppModel) {
		stop()
		monitor = NSEvent.addLocalMonitorForEvents(matching: .pressure) { [weak self, weak app] event in
			// The event is read here, on the main thread; only plain values cross into the actor.
			let stage = event.stage
			let location = event.locationInWindow
			let contentHeight = event.window?.contentView?.bounds.height
			MainActor.assumeIsolated {
				self?.handle(stage: stage, location: location, contentHeight: contentHeight, app: app)
			}
			return event
		}
	}

	private func handle(stage: Int, location: CGPoint, contentHeight: CGFloat?, app: AppModel?) {
		let isForce = stage >= 2
		defer { wasForceClicking = isForce }
		guard isForce, !wasForceClicking, let app, let contentHeight else { return }
		// SwiftUI's global space has its origin at the window's top left.
		let point = CGPoint(x: location.x, y: contentHeight - location.y)
		if let subject = app.peekTargets.subject(at: point) {
			app.peek.show(subject)
		}
	}

	func stop() {
		if let monitor { NSEvent.removeMonitor(monitor) }
		monitor = nil
	}
}
