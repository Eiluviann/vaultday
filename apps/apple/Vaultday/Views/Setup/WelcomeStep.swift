import SwiftUI

/// Step 1: what Vaultday is, for work and for life. No AI card: the base
/// version has no AI, so the board's third card is left out.
struct WelcomeStep: View {
	private let features: [(String, String)] = [
		("Notes", "doc.text"), ("Tasks", "checkmark.circle"), ("Reminders", "bell"), ("Calendar", "calendar"),
		("Mail", "envelope"), ("Timer", "timer"), ("Routines", "repeat"), ("Projects", "folder"),
	]
	private let devices: [(String, String)] = [
		("Mac", "laptopcomputer"), ("iPhone", "iphone"), ("iPad", "ipad"), ("Apple Watch", "applewatch"),
	]

	var body: some View {
		StepPane(title: "Everything in one app", lede: "Notes, tasks, reminders, calendar, mail and more, in one place for work and for life. One day plan holds all of it") {
			Image(nsImage: NSApplication.shared.applicationIconImage)
				.resizable()
				.frame(width: 64, height: 64)
				.accessibilityLabel("Vaultday app icon")
			FlowRow(spacing: 6) {
				ForEach(features, id: \.0) { feature in
					HStack(spacing: 6) {
						Image(systemName: feature.1).foregroundStyle(Tokens.Colors.primary)
						Text(feature.0).font(Tokens.Typography.callout.font.weight(.bold))
					}
					.foregroundStyle(Tokens.Colors.ink)
					.padding(.leading, 8)
					.padding(.trailing, 12)
					.padding(.vertical, 6)
					.background(Tokens.Colors.surfaceSunken, in: Capsule())
				}
			}
			madeForApple
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				Eyebrow(text: "For work and for life, kept apart where it matters")
				HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
					infoCard(.work, icon: "briefcase", title: "Work", text: "Tickets, meetings, work mail and hours. Shown on a shared screen one workspace at a time")
					infoCard(.life, icon: "leaf", title: "Life", text: "Errands, routines, the family calendar and personal projects, planned into evenings and weekends")
				}
			}
		}
	}

	private var madeForApple: some View {
		HStack(spacing: Tokens.Spacing.space12) {
			Eyebrow(text: "Made for Apple")
			ForEach(devices, id: \.0) { device in
				HStack(spacing: 6) {
					Image(systemName: device.1)
					Text(device.0).font(Tokens.Typography.body.font.weight(.semibold))
				}
				.foregroundStyle(Tokens.Colors.ink)
			}
			Spacer(minLength: 0)
			HStack(spacing: 6) {
				Image(systemName: "pc")
				Text("Windows and Linux").font(Tokens.Typography.body.font.weight(.semibold))
			}
			.foregroundStyle(Tokens.Colors.inkMuted)
			PlannedTag()
		}
		.padding(Tokens.Spacing.space12)
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line, lineWidth: 1))
	}

	private func infoCard(_ tone: SetupTag.Tone, icon: String, title: String, text: String) -> some View {
		let tint = tone == .work ? Tokens.Colors.work : Tokens.Colors.life
		return VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space8) {
				Image(systemName: icon)
				Text(title).font(Tokens.Typography.headline.font)
			}
			.foregroundStyle(tint)
			Text(text).font(Tokens.Typography.callout.font).foregroundStyle(Tokens.Colors.inkSecondary)
		}
		.padding(Tokens.Spacing.space16)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(tone == .work ? Tokens.Colors.workSoft : Tokens.Colors.lifeSoft, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.accessibilityElement(children: .combine)
	}
}

/// Items laid out left to right, wrapping to the next line.
struct FlowRow: Layout {
	var spacing: CGFloat = 6

	func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
		arrange(in: proposal.width ?? .infinity, subviews: subviews).size
	}

	func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
		let arranged = arrange(in: bounds.width, subviews: subviews)
		for (index, origin) in arranged.origins.enumerated() {
			subviews[index].place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), proposal: .unspecified)
		}
	}

	private func arrange(in width: CGFloat, subviews: Subviews) -> (size: CGSize, origins: [CGPoint]) {
		var origins: [CGPoint] = []
		var x: CGFloat = 0
		var y: CGFloat = 0
		var rowHeight: CGFloat = 0
		var maxX: CGFloat = 0
		for subview in subviews {
			let size = subview.sizeThatFits(.unspecified)
			if x > 0, x + size.width > width {
				x = 0
				y += rowHeight + spacing
				rowHeight = 0
			}
			origins.append(CGPoint(x: x, y: y))
			x += size.width + spacing
			rowHeight = max(rowHeight, size.height)
			maxX = max(maxX, x - spacing)
		}
		return (CGSize(width: maxX, height: y + rowHeight), origins)
	}
}
