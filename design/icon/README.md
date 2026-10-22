# Vaultday app icon

**Vault sunrise, socket bolts, slate.** A round vault opening with the sun rising in it. The rim carries eight socket bolts. Chosen on 17 Oct 2026 after three rounds of concepts.

## Files

| Path | What it is |
| --- | --- |
| `layers/0-background.svg` | Background: the slate gradient, full bleed 1024 × 1024 |
| `layers/1-sunrise.svg` | Foreground 1: the sky disc, the sun and the ground, clipped to the opening |
| `layers/2-bolted-rim.svg` | Foreground 2: the rim and its eight socket bolts |
| `vaultday-icon-master.svg` | Preview composite of all three layers, clipped to the icon shape. For mockups only; Icon Composer adds the real shape |
| `png/vaultday-icon-{1024…32}.png` | Rasterised composite, for docs and mockups |
| `png/layer-*.png` | Each layer at 1024, for tools that don't take SVG |
| `glyphs/menubar-template.svg` | Menu-bar template image, black and alpha only, 18 pt |
| `glyphs/watch-complication.svg` | Watch complication glyph in white, which watchOS tints |
| `glyphs/glyph-sockets-large.svg` | Single-colour glyph with the bolts punched through. For 32 pt and up only |
| `glyphs/in-app-porthole.svg` | In-app mark: sunrise and rim without the background tile, rim in slate `#46586C`, plain bolts `#F1EEE8` r 22 with no socket. Used in the onboarding sidebar and the Welcome hero at 34 and 64 pt |

All geometry is drawn by hand on a 1024 grid with its centre at (512, 512); there are no SF Symbols in it. Opening radius 290, rim stroke 84, bolts at radius 290 every 45°, bolt face r 32 with socket r 13.

## Palette

| Role | Default | Dark |
| --- | --- | --- |
| Background top / bottom | `#3A4E63` / `#1B2633` | `#2A2F37` / `#0B0D10` |
| Sky top / bottom | `#FFB547` / `#FF7A3D` | same |
| Sun and rim | `#F1EEE8` | `#D9DCE1` sun, `#E4E6EA` rim |
| Bolt face | `#7F93A8` | `#5B6470` |
| Ground and socket | `#1B2633` (socket at 55% opacity) | `#14171B` |

Tinted and Clear come from Icon Composer's own monochrome and glass treatments; the layers need no changes for them.

## Building the icon (Phase 1)

1. Open Icon Composer and create `Vaultday.icon`.
2. Set the background fill to the default gradient above, or drop in `0-background.svg`.
3. Add `1-sunrise.svg` and `2-bolted-rim.svg` as two separate layers, in that order, so the glass and specular passes treat the rim as its own raised piece.
4. In the Dark appearance, override the background gradient and the rim and sun fills with the Dark column above.
5. Save `Vaultday.icon` into `apps/mac` (and the shared target) once the Xcode project exists.

## Glyphs

- **Menu bar:** `menubar-template.svg` in the asset catalog, with Render As set to Template Image and Preserve Vector Data turned on. The bolts are left out on purpose: at 16–18 pt they close up and the ring starts to read as a cog.
- **Watch:** `watch-complication.svg` for circular and corner complications, and in Smart Stack cards.
- **Large single-colour uses** (widgets, the About window, onboarding): `glyph-sockets-large.svg`.

## History

- Round 1, 20 concepts: https://claude.ai/artifact/Lo5YZR4gZiJqbsZxrDCGN9
- Round 2, A2 / C1 / C2 / D2: https://claude.ai/artifact/VEN3UvLJiA4r6vt1bZRKU5
- Round 3, A2 bolt treatments: https://claude.ai/artifact/9te6yWJWYBjnMDEa1UA9wi

These pages are private to the owner.
