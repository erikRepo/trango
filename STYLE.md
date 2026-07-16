# Design reference & style guide

This file holds the original visual/design-reference material from the
product handoff — split out of the root `README.md` to keep that file
short. See [SPEC.md](SPEC.md) for the functional handoff spec (screens,
interactions, state) and the [docs/](https://erikrepo.github.io/trango/)
mdBook for how the app actually behaves today.

## About the Design Files

The file in this bundle (`design_reference.dc.html`) is a **design reference built in HTML** — a static visual mockup showing intended look, layout, and states. It is NOT production code and should not be ported directly. Your task is to **recreate this design natively in Rust + Slint**, using `.slint` markup for layout/styling and Rust for state/logic, with `libmpv` for video playback and subtitle parsing/timing.

Open `design_reference.dc.html` in any browser to view it (it needs `support.js` alongside it, included in this folder). It contains two labeled sections:
- **id="1c"** — the main player screen
- **id="2a"** — two dialog states: "Open video" file picker, and "Open subtitles" panel (including the "no subtitles found → Generate subtitles" state)

## Fidelity
**High-fidelity.** Colors, spacing, type sizes, and copy in the mock are intentional — match them closely. Treat placeholder video frames (diagonal-stripe pattern) as a stand-in for the actual libmpv render surface.

## Design Tokens

**Colors (dark theme, oklch → approximate hex):**
- Window background: `oklch(0.16 0.005 260)` ≈ `#1c1d22`
- Panel / topbar background: `oklch(0.18 0.005 260)` ≈ `#202127`
- Card / list background: `oklch(0.16–0.19 0.005 260)` ≈ `#1c1d22`–`#22242b`
- Borders: `oklch(0.28–0.32 0.005 260)` ≈ `#3a3c45`
- Accent (primary, buttons/toggle/progress): `oklch(0.5 0.16 250)` ≈ `#3b6fd6` (blue)
- Accent tint (highlighted rows): `oklch(0.28 0.06 250)` ≈ `#2c3550`
- Translation text accent: `oklch(0.68 0.13 250)` ≈ `#7fa6f0`
- Primary text: `oklch(0.9–0.94 0.005 260)` ≈ `#e7e8ec`
- Muted/secondary text: `oklch(0.5–0.65 0.01 260)` ≈ `#8a8d99`
- Modal backdrop: `rgba(0,0,0,0.55)`

**Typography:**
- UI text: Inter, weights 400/500/600/700
- Monospace (timestamps, labels, hints, file metadata): JetBrains Mono, weights 400/500
- Sizes: sentence text 24px/600 (current), 18px/500 (translation); body/buttons 12–13px/500–600; uppercase micro-labels 10px/600 with 0.8px letter-spacing; hint bar 11px/500

**Radii:** 6–8px for buttons/cards/rows, 12px for modals, full-round for toggles/scrub thumb/play button.

**Shadows:** modal `0 24px 60px rgba(0,0,0,0.6)`; outer window `0 20px 60px rgba(0,0,0,0.5)`.

**Spacing:** 16px outer margins around video frame; 12–14px gaps between stacked panel cards; 8–10px internal row padding.

## Assets
No external image assets — the mockup uses solid-color chips as file-type icons and a diagonal-stripe CSS pattern as a video-frame placeholder. Source real icons (video/subtitle file-type glyphs) and the actual libmpv render target when implementing.

## Files
- `design_reference.dc.html` — the visual reference (open in a browser; requires `support.js` in the same folder)
- `support.js` — runtime the reference file depends on to render; not related to your implementation
