import SwiftUI
import VaultdayCore

// The design system's components (Button, Tag, Callout, Card, SpherePanel,
// WorkspaceBadge, ChoiceCard, ScopeChip, SegmentedControl, Kbd), as SwiftUI,
// in the design tokens. Setup drew them first; every screen uses them.

// MARK: Button

/// Primary, secondary and plain buttons, regular (28 px) and small (22 px).
struct SetupButtonStyle: ButtonStyle {
	enum Kind { case primary, secondary, plain }

	var kind: Kind = .secondary
	var isSmall = false
	@Environment(\.isEnabled) private var isEnabled

	func makeBody(configuration: Configuration) -> some View {
		configuration.label
			.font(isSmall ? Tokens.Typography.callout.font.weight(.medium) : Tokens.Typography.body.font.weight(kind == .primary ? .semibold : .medium))
			// A label is never cut short: the button takes the width its label needs, and the row gives way around it.
			.lineLimit(1)
			.fixedSize(horizontal: true, vertical: false)
			.padding(.horizontal, isSmall || kind == .plain ? Tokens.Spacing.space8 : Tokens.Spacing.space16)
			.frame(height: isSmall ? Tokens.Size.sizeControlSm : Tokens.Size.sizeControl)
			.foregroundStyle(foreground)
			.background(background(isPressed: configuration.isPressed), in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
			.overlay {
				if kind == .secondary, isEnabled {
					RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm)
						.strokeBorder(configuration.isPressed ? Tokens.Colors.lineButtonHover : Tokens.Colors.lineButton, lineWidth: 1)
				}
			}
			.contentShape(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
	}

	private var foreground: Color {
		guard isEnabled else { return Tokens.Colors.inkDisabled }
		return kind == .primary ? Tokens.Colors.onPrimary : Tokens.Colors.ink
	}

	private func background(isPressed: Bool) -> Color {
		guard isEnabled else { return kind == .plain ? .clear : Tokens.Colors.fillControl }
		switch kind {
		case .primary: return isPressed ? Tokens.Colors.primaryHover : Tokens.Colors.primary
		case .secondary: return isPressed ? Tokens.Colors.fillControl : Tokens.Colors.surfaceRaised
		case .plain: return isPressed ? Tokens.Colors.fillHover : .clear
		}
	}
}

/// A label with an optional leading or trailing SF Symbol, 16 px.
struct SetupLabel: View {
	let title: String
	var icon: String?
	var iconAfter: String?

	var body: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			if let icon { Image(systemName: icon).iconGlyph(.row) }
			if !title.isEmpty { Text(title) }
			if let iconAfter { Image(systemName: iconAfter).iconGlyph(.row) }
		}
	}
}

// MARK: Tag, callout, key cap

/// A pill: state ("Active"), a warning, a sphere, or "Planned".
struct SetupTag: View {
	enum Tone { case neutral, on, caution, work, life }

	let text: String
	var tone: Tone = .neutral
	var icon: String?

	var body: some View {
		HStack(spacing: Tokens.Spacing.space4) {
			if let icon { Image(systemName: icon).iconGlyph(.tag, weight: .semibold) }
			Text(text)
		}
		.textStyle(Tokens.Typography.caption).fontWeight(.semibold)
		.padding(.horizontal, Tokens.Spacing.space8)
		.frame(height: 20)
		.foregroundStyle(foreground)
		.background(background, in: Capsule())
		.fixedSize()
	}

	private var foreground: Color {
		switch tone {
		case .neutral: Tokens.Colors.inkSecondary
		case .on: Tokens.Colors.positive
		case .caution: Tokens.Colors.caution
		case .work: Tokens.Colors.work
		case .life: Tokens.Colors.life
		}
	}

	private var background: Color {
		switch tone {
		case .neutral: Tokens.Colors.fillControl
		case .on: Tokens.Colors.positiveSoft
		case .caution: Tokens.Colors.cautionSoft
		case .work: Tokens.Colors.workSoft
		case .life: Tokens.Colors.lifeSoft
		}
	}
}

/// The "Planned" tag: the setting is stored, nothing acts on it yet.
struct PlannedTag: View {
	var body: some View { SetupTag(text: "Planned") }
}

/// A note under the lede: sunken, with an icon, full sentences without a
/// closing period. The icon sits on the text's first line, sharing its
/// centre line, however many lines the text runs to.
struct SetupCallout<Action: View>: View {
	enum Tone { case neutral, caution }

	var tone: Tone = .neutral
	var icon = "info.circle"
	let text: String
	@ViewBuilder var action: Action

	var body: some View {
		HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space8) {
			// A symbol set in the text's own font stands on the same baseline and centres on its capitals.
			Image(systemName: icon)
				.textStyle(Tokens.Typography.callout)
				.frame(width: IconBox.row.size)
				.foregroundStyle(tone == .caution ? Tokens.Colors.caution : Tokens.Colors.inkMuted)
			Text(text)
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(tone == .caution ? Tokens.Colors.ink : Tokens.Colors.inkSecondary)
				.frame(maxWidth: .infinity, alignment: .leading)
			action
		}
		.padding(.horizontal, Tokens.Spacing.space12)
		.padding(.vertical, Tokens.Spacing.space8)
		.background(tone == .caution ? Tokens.Colors.cautionSoft : Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.accessibilityElement(children: .combine)
	}
}

extension SetupCallout where Action == EmptyView {
	init(tone: Tone = .neutral, icon: String = "info.circle", text: String) {
		self.init(tone: tone, icon: icon, text: text) { EmptyView() }
	}
}

/// A keyboard cap.
struct KbdCap: View {
	let text: String

	var body: some View {
		Text(text)
			.textStyle(Tokens.Typography.kbd)
			.foregroundStyle(Tokens.Colors.ink)
			.padding(.horizontal, 5)
			.padding(.vertical, 2)
			.frame(minWidth: 14)
			.background(Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
	}
}

// MARK: Segmented control

/// A track of segments; the chosen one is raised. The track takes the ground
/// it sits on: soft on white, the control fill on sunken and sidebar grounds.
struct SegmentedChoice<Value: Hashable>: View {
	enum Ground { case white, sunken }

	let label: String
	let options: [(value: Value, title: String)]
	@Binding var selection: Value
	var ground: Ground = .white
	var fills = false

	var body: some View {
		HStack(spacing: Tokens.Spacing.space2) {
			ForEach(options.indices, id: \.self) { index in
				let option = options[index]
				let isSelected = option.value == selection
				Button {
					selection = option.value
				} label: {
					Text(option.title)
						.font(Tokens.Typography.callout.font.weight(isSelected ? .semibold : .regular))
						.foregroundStyle(isSelected ? Tokens.Colors.ink : Tokens.Colors.inkSecondary)
						.lineLimit(1)
						.padding(.horizontal, Tokens.Spacing.space6)
						.frame(maxWidth: fills ? .infinity : nil)
						.frame(height: Tokens.Size.sizeControlSm)
						.background {
							if isSelected {
								RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm)
									.fill(Tokens.Colors.surfaceRaised)
									.shadow(color: .black.opacity(0.12), radius: 1, y: 1)
							}
						}
						.contentShape(Rectangle())
				}
				.buttonStyle(.plain)
				.accessibilityAddTraits(isSelected ? .isSelected : [])
			}
		}
		.padding(Tokens.Spacing.space2)
		.background(ground == .sunken ? Tokens.Colors.fillControl : Tokens.Colors.fillControlSoft, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
		.accessibilityElement(children: .contain)
		.accessibilityLabel(label)
	}
}

// MARK: Card

/// A setting row: icon, title, a tag, a description and the control pinned right.
struct SetupCard<Trailing: View, Extra: View>: View {
	var icon: String?
	/// A workspace card's badge, in the icon box on the title line.
	var badge: WorkspaceBadge?
	var iconTone: SetupTag.Tone = .neutral
	let title: String
	var description: String?
	var tags: [SetupTag] = []
	var footnote: String?
	var footnoteTone: SetupTag.Tone = .neutral
	@ViewBuilder var trailing: Trailing
	@ViewBuilder var extra: Extra

	var body: some View {
		VStack(spacing: 0) {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
				HStack(spacing: Tokens.Spacing.space8) {
					if let badge {
						badge
							.frame(width: IconBox.menu.size, height: IconBox.menu.size)
							.padding(.leading, Tokens.Spacing.space4)
					} else if let icon {
						Image(systemName: icon)
							.iconBox(.menu)
							.foregroundStyle(tint)
							.padding(.leading, Tokens.Spacing.space4)
					}
					Text(title)
						.textStyle(Tokens.Typography.headline)
						.foregroundStyle(Tokens.Colors.ink)
						.lineLimit(1)
						.layoutPriority(-1)
					ForEach(tags.indices, id: \.self) { tags[$0] }
					Spacer(minLength: Tokens.Spacing.space12)
					trailing
				}
				.frame(minHeight: Tokens.Size.sizeRow)
				// The body starts at the icon box's left edge, 16 pt in, so the icon anchors the column.
				if let description {
					Text(description)
						.textStyle(Tokens.Typography.callout)
						.foregroundStyle(Tokens.Colors.inkSecondary)
						.padding(.leading, hasIconBox ? Tokens.Spacing.space4 : 0)
						.frame(maxWidth: .infinity, alignment: .leading)
				}
				extra
					.padding(.leading, hasIconBox ? Tokens.Spacing.space4 : 0)
			}
			.padding(Tokens.Spacing.space12)
			.frame(maxWidth: .infinity, alignment: .leading)
			if let footnote {
				Text(footnote)
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(footnoteColor)
					.padding(.horizontal, Tokens.Spacing.space16)
					.padding(.vertical, Tokens.Spacing.space8)
					.frame(maxWidth: .infinity, alignment: .leading)
					.background(footnoteBackground)
			}
		}
		.background(Tokens.Colors.surfaceRaised)
		.clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line, lineWidth: 1))
	}

	private var hasIconBox: Bool { badge != nil || icon != nil }

	private var tint: Color {
		switch iconTone {
		case .work: Tokens.Colors.work
		case .life: Tokens.Colors.life
		default: Tokens.Colors.inkMuted
		}
	}

	private var footnoteColor: Color { footnoteTone == .caution ? Tokens.Colors.caution : Tokens.Colors.inkSecondary }
	private var footnoteBackground: Color { footnoteTone == .caution ? Tokens.Colors.cautionSoft : Tokens.Colors.surfaceSunken }
}

extension SetupCard where Extra == EmptyView {
	init(icon: String? = nil, badge: WorkspaceBadge? = nil, iconTone: SetupTag.Tone = .neutral, title: String, description: String? = nil, tags: [SetupTag] = [], footnote: String? = nil, footnoteTone: SetupTag.Tone = .neutral, @ViewBuilder trailing: () -> Trailing) {
		self.init(icon: icon, badge: badge, iconTone: iconTone, title: title, description: description, tags: tags, footnote: footnote, footnoteTone: footnoteTone, trailing: trailing) { EmptyView() }
	}
}

extension SetupCard where Trailing == EmptyView {
	init(icon: String? = nil, badge: WorkspaceBadge? = nil, iconTone: SetupTag.Tone = .neutral, title: String, description: String? = nil, tags: [SetupTag] = [], @ViewBuilder extra: () -> Extra) {
		self.init(icon: icon, badge: badge, iconTone: iconTone, title: title, description: description, tags: tags, trailing: { EmptyView() }, extra: extra)
	}
}

// MARK: Sphere panel, badge, chips, choice cards

/// The Work, Life or plain group panel: a header and the cards inside it.
/// The header's icon tile is the sphere's soft tint with a hairline of the
/// sphere color, as the design system edges its tinted notes, so the tile
/// stands off the sunken panel.
struct SpherePanel<Action: View, Content: View>: View {
	let sphere: WorkspaceSphere
	@ViewBuilder var action: Action
	@ViewBuilder var content: Content

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			HStack(spacing: Tokens.Spacing.space8) {
				Image(systemName: sphere == .work ? "briefcase" : "leaf")
					.iconGlyph(.row)
					.frame(width: SpherePanelMetrics.tileSize, height: SpherePanelMetrics.tileSize)
					.foregroundStyle(tint)
					.background(soft, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
					.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(tint.opacity(SpherePanelMetrics.tileEdgeOpacity), lineWidth: 1))
				Text(sphere == .work ? "Work" : "Life")
					.textStyle(Tokens.Typography.heading)
					.foregroundStyle(tint)
				Spacer()
				action
			}
			content
		}
		.padding(Tokens.Spacing.space12)
		.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXl))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusXl).strokeBorder(Tokens.Colors.line, lineWidth: 1))
	}

	private var tint: Color { sphere == .work ? Tokens.Colors.work : Tokens.Colors.life }
	private var soft: Color { sphere == .work ? Tokens.Colors.workSoft : Tokens.Colors.lifeSoft }
}

/// The sphere panel's mark (SpherePanel guideline: a 22 pt tile, radius-sm).
enum SpherePanelMetrics {
	static let tileSize: CGFloat = 22
	/// The tile's hairline: the sphere color at the strength of the info footnote's edge.
	static let tileEdgeOpacity = 0.35
}

/// A workspace's identity color, from the accent word in its manifest.
enum WorkspaceColor: String, CaseIterable, Identifiable {
	case blue, plum, green, copper, graphite

	var id: String { rawValue }

	var color: Color {
		switch self {
		case .blue: Tokens.Colors.wsBlue
		case .plum: Tokens.Colors.wsPlum
		case .green: Tokens.Colors.wsGreen
		case .copper: Tokens.Colors.wsCopper
		case .graphite: Tokens.Colors.wsGraphite
		}
	}

	/// The color a workspace shows: its accent word, else its sphere's default; a word this build does not know reads as graphite.
	static func resolve(accent: String?, sphere: WorkspaceSphere) -> WorkspaceColor {
		guard let accent, !accent.isEmpty else { return sphere == .work ? .blue : .green }
		return WorkspaceColor(rawValue: accent) ?? .graphite
	}
}

/// The numbered badge of a workspace: its ⌘ number, centred on a rounded
/// square of its color; 16 pt in a card's icon box, 14 pt in lists.
struct WorkspaceBadge: View {
	let number: Int
	let color: WorkspaceColor
	var isLarge = true

	var body: some View {
		NumberMark(number: number, metrics: isLarge ? .badgeLarge : .badge, foreground: Tokens.Colors.onWs, fill: color.color)
			.accessibilityElement()
			.accessibilityLabel("Workspace \(number)")
	}
}

/// A pill the scope row of a feature tile ticks: a sphere or a workspace.
struct ScopeChip: View {
	let title: String
	var number: Int?
	var color: WorkspaceColor?
	var sphere: WorkspaceSphere?
	let isOn: Bool
	let toggle: () -> Void

	var body: some View {
		Button(action: toggle) {
			HStack(spacing: Tokens.Spacing.space6) {
				if let sphere {
					Image(systemName: sphere == .work ? "briefcase" : "leaf").iconGlyph(.row)
				} else if let number, let color {
					NumberMark(number: number, metrics: .badge, outline: .circle, foreground: isOn ? Tokens.Colors.onWs : Tokens.Colors.inkMuted, fill: isOn ? color.color : Tokens.Colors.fillControl)
				}
				Text(title)
			}
			.textStyle(Tokens.Typography.callout).fontWeight(.semibold)
			.foregroundStyle(isOn ? Tokens.Colors.ink : Tokens.Colors.inkMuted)
			.padding(.horizontal, Tokens.Spacing.space8)
			.frame(height: Tokens.Size.sizeControlSm)
			.background(isOn ? Tokens.Colors.surfaceSunken : Tokens.Colors.surfaceRaised, in: Capsule())
			.overlay(Capsule().strokeBorder(isOn ? Tokens.Colors.inkMuted : Tokens.Colors.lineControl, lineWidth: 1))
		}
		.buttonStyle(.plain)
		.accessibilityAddTraits(isOn ? .isSelected : [])
	}
}

/// A card that is one choice of several.
struct ChoiceCard: View {
	let icon: String
	let title: String
	let description: String
	var tag: String?
	let isSelected: Bool
	let choose: () -> Void

	var body: some View {
		Button(action: choose) {
			HStack(alignment: .top, spacing: Tokens.Spacing.space8) {
				Image(systemName: isSelected ? "largecircle.fill.circle" : "circle")
					.foregroundStyle(isSelected ? Tokens.Colors.primary : Tokens.Colors.lineControl)
				VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
					HStack(spacing: Tokens.Spacing.space6) {
						Image(systemName: icon).foregroundStyle(Tokens.Colors.inkMuted)
						Text(title).textStyle(Tokens.Typography.body).fontWeight(.semibold).foregroundStyle(Tokens.Colors.ink)
					}
					Text(description)
						.textStyle(Tokens.Typography.callout)
						.foregroundStyle(Tokens.Colors.inkSecondary)
						.frame(maxWidth: .infinity, alignment: .leading)
					if let tag {
						SetupTag(text: tag, tone: .on).padding(.top, Tokens.Spacing.space8)
					}
				}
			}
			.padding(Tokens.Spacing.space12)
			.frame(maxWidth: .infinity, alignment: .topLeading)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
			.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(isSelected ? Tokens.Colors.primary : Tokens.Colors.line, lineWidth: isSelected ? 2 : 1))
		}
		.buttonStyle(.plain)
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}
}

// MARK: Row options

/// A row's options: an icon-only small secondary key with the vertical
/// ellipsis (Button guideline), never a bare glyph; the menu opens from it.
struct RowOptionsMenu<Items: View>: View {
	let label: String
	@ViewBuilder var items: Items

	var body: some View {
		Menu {
			items
		} label: {
			Image(systemName: "ellipsis")
				.iconGlyph(.row, weight: .semibold)
				.rotationEffect(.degrees(90))
				.foregroundStyle(Tokens.Colors.ink)
				.frame(width: Tokens.Size.sizeControlSm, height: Tokens.Size.sizeControlSm)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton, lineWidth: 1))
				.contentShape(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
		}
		.menuStyle(.button)
		.buttonStyle(.plain)
		.menuIndicator(.hidden)
		.fixedSize()
		.accessibilityLabel(label)
	}
}

// MARK: Step pane

/// A step's body: the title, the lede, and the content, scrolling above the
/// footer. The content keeps to the board's content width and to the left,
/// however wide the window is; ledes stop at the measure.
struct StepPane<Content: View>: View {
	let title: String
	let lede: String
	@ViewBuilder var content: Content

	var body: some View {
		ScrollView {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
				VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
					Text(title)
						.textStyle(Tokens.Typography.titleLarge)
						.foregroundStyle(Tokens.Colors.ink)
						.accessibilityAddTraits(.isHeader)
					Text(lede)
						.textStyle(Tokens.Typography.body)
						.foregroundStyle(Tokens.Colors.inkSecondary)
						.frame(maxWidth: SetupLayout.measure, alignment: .leading)
				}
				content
			}
			.frame(maxWidth: SetupLayout.contentMaxWidth, alignment: .leading)
			.padding(.horizontal, SetupLayout.gutter)
			.padding(.top, SetupLayout.gutter)
			.padding(.bottom, Tokens.Spacing.space16)
			.frame(maxWidth: .infinity, alignment: .leading)
		}
	}
}

/// A label over a list: the uppercase eyebrow.
struct Eyebrow: View {
	let text: String

	var body: some View {
		Text(text.uppercased())
			.textStyle(Tokens.Typography.eyebrow)
			.foregroundStyle(Tokens.Colors.inkMuted)
	}
}
