# 13 — Art and Asset Pipeline

## Purpose

Lists every art asset the app needs, with technical specs, file naming, folder structure, export settings and placeholder rules. Appearance is left to the author.

## Scope

- **Covers:** Original art bundled with the app, store and installer images, and the handling of Valve game art.
- **Elsewhere:** Data, text and audio assets are in [14](14-data-and-content-assets.md).

## Details

### Valve game art (not bundled)

- **What:** Hero portraits and item icons.
- **Source:** Loaded at runtime from deadlock-api assets (API-15) and cached per patch in `<data>/assets/<patch>/`.
- **Never** bundled in the Flatpak, installer, repository or store screenshots beyond normal app use.
- **Why:** Valve's content license is personal and non-commercial ([03](03-fair-play-and-compliance.md)).
- **Original assets:** Original icons for game concepts (soul sources, damage sources) must not copy Valve's art.

### Asset list

All statuses are **Not started**. Priority is when the asset is first needed ([17](17-roadmap.md)).

| ID | Name | Type | Dimensions / format | Count | Usage | Priority |
|---|---|---|---|---|---|---|
| AST-ICON-001 | App icon master | Vector | SVG, square viewBox `0 0 512 512` | 1 | Source for all icon exports | Alpha |
| AST-ICON-002 | App icon PNG exports | Raster | PNG sRGB RGBA at 16, 24, 32, 48, 64, 128, 256, 512 px | 8 | Flatpak hicolor theme, window icon | Alpha |
| AST-ICON-003 | Windows icon | Raster | ICO with 16, 24, 32, 48, 64, 256 px layers | 1 | Windows executable and installer | Beta |
| AST-ICON-004 | Symbolic icon | Vector | SVG, 16 × 16 viewBox, single color | 1 | Linux symbolic contexts [ASSUMPTION] | Beta |
| AST-MASCOT-001 | Mascot master | Vector or layered raster | SVG, or PSD/KRA at 2048 × 2048 plus PNG 1024 × 1024 | 1 | About view, store listing | Beta |
| AST-MASCOT-002 | Mascot empty-state variants | Vector | SVG, 512 × 512 viewBox | 4 (no data, offline, error, no reviews yet) [ASSUMPTION count] | Empty and error states ([09](09-ui-window-and-overlay.md)) | v1 |
| AST-UI-001 | Review tier icons | Vector | SVG, 24 × 24 grid, 2 px safe margin | 9 (T1–T9) | Review lists and Key Moments | Beta |
| AST-UI-002 | Flag icons | Vector | SVG, 16 × 16 grid | 3 (smurf, stack, met before) | Scouting Cards | v1 |
| AST-UI-003 | Status icons | Vector | SVG, 16 × 16 grid | 4 (live, stale, lost, offline) | OV-5, Home view | Beta |
| AST-UI-004 | Phase markers | Vector | SVG, 16 × 16 grid | 3 (lane, mid, late) | Review, OV-1 | Beta |
| AST-UI-005 | Soul source icons | Vector | SVG, 16 × 16 grid | 5 (lane troopers, neutral camps, breakables, Urn, Sinner's Sacrifice) | Review souls section | Alpha |
| AST-UI-006 | Damage source icons | Vector | SVG, 16 × 16 grid | 3 (heroes, troopers, neutrals) | Review damage section | Beta |
| AST-UI-007 | Overlay panel icons | Vector | SVG, 16 × 16 grid | 4 (counter items, swap, pace, next objective) | OV-1 to OV-4 | Beta |
| AST-UI-008 | Setup check icons | Vector | SVG, 16 × 16 grid | 3 (pass, fail, pending) | Home view | Alpha |
| AST-STORE-001 | Store screenshots | Raster | PNG, 1920 × 1080 [VERIFY Flathub guidance] | ≥ 3 | Flathub listing, GitHub | v1 |
| AST-STORE-002 | Installer banner | Raster | BMP, 493 × 58 (WiX banner) | 1 | Windows installer | Beta |
| AST-STORE-003 | Installer dialog image | Raster | BMP, 493 × 312 (WiX dialog) | 1 | Windows installer | Beta |

### Technical specs

| Format | Rules |
|---|---|
| SVG | SVG 1.1. No embedded raster images. Text converted to paths. No external references. Square viewBox. Colors set so the UI can theme icons through `fill="currentColor"` where icons are single-color [ASSUMPTION] |
| PNG | sRGB, 8 bits per channel, RGBA, no interlacing, optimized with `oxipng -o 4` |
| ICO | Layers as listed. PNG-compressed 256 px layer |
| BMP | 24-bit, as required by WiX |
| Pixel grid | Icons drawn on whole-pixel coordinates at 1× of their grid |

### File naming

`ast-<category>-<nnn>-<slug>[-<size>].<ext>`, all lowercase, hyphens only.

| Example | Asset |
|---|---|
| `ast-icon-001-app.svg` | AST-ICON-001 |
| `ast-icon-002-app-128.png` | AST-ICON-002, 128 px |
| `ast-ui-001-tier-t7.svg` | AST-UI-001, tier T7 |
| `ast-ui-005-soul-urn.svg` | AST-UI-005, Urn |

### Folder structure

```text
assets/
├── source/            # master files (SVG, PSD/KRA); not packaged
│   ├── icon/
│   └── mascot/
├── icon/              # AST-ICON-*
├── mascot/            # AST-MASCOT-*
├── ui/                # AST-UI-*
├── store/             # AST-STORE-*
└── placeholders/      # generated; see below
```

### Export pipeline

1. Edit masters in `assets/source/`.
2. `just export-assets` (task T-030) runs:
    - `resvg` to render AST-ICON-002 PNGs from AST-ICON-001.
    - `oxipng -o 4` on all PNGs.
    - ICO assembly for AST-ICON-003.
    - SVG checks: no `<image>`, no `<text>`, no external `href`.
3. CI fails if a listed asset is missing or breaks a rule above (TST-110).

### Placeholders

Until final art exists, every asset is a generated placeholder:

- A 50% gray (`#808080`) square at the asset's size, with a 1 px darker border.
- The asset ID centered in a sans-serif font at the largest size that fits.
- Filename as final, with `.placeholder` before the extension (for example `ast-ui-001-tier-t7.placeholder.svg`).
- The build warns, but doesn't fail, while placeholders remain. Release builds for v1 fail if any remain (TST-110).

### Creative decisions

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-10 — App icon design and the icon style shared by all AST-UI icons
> Constraints from the project: Must read at 16 px. Single-color icons must work with `currentColor` theming. Must not use Valve's art or logos, or imply Valve endorsement.
> Options to consider (optional): —
> Decision: ____________

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-11 — Mascot appearance. The concept is fixed by the author: "monke with a Tesla cannon... from someplace really cool and interesting... like the moon."
> Constraints from the project: Must work as an SVG at 512 × 512 and as a PNG at 1024 × 1024. Must not resemble any Deadlock hero or other protected character.
> Options to consider (optional): —
> Decision: ____________

Tier icon design is part of CD-07 ([07](07-post-match-review-engine.md)).

## Dependencies

- [03 Fair play](03-fair-play-and-compliance.md): Valve content license.
- [07 Review](07-post-match-review-engine.md): CD-07, tier list.
- [09 UI](09-ui-window-and-overlay.md): where assets appear.
- [15 Tools](15-tools-build-and-deploy.md): `just export-assets`, packaging.
- [17 Roadmap](17-roadmap.md)

## Open Questions

- What are Flathub's current screenshot and icon requirements? [VERIFY]
- [ASSUMPTION] AST-ICON-004 symbolic icon, 4 mascot variants, `currentColor` theming.

## Acceptance Criteria

- [ ] Every AST ID exists as a file (final or placeholder) at the listed path and size.
- [ ] TST-110 passes: no SVG contains `<image>`, `<text>` or external references.
- [ ] No Valve art is present in the repository or the packages.
- [ ] No placeholders remain in the v1 release build.
