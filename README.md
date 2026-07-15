# Handoff: Language-Learning Video Player (Rust + Slint + libmpv)

## Overview
A desktop video player for language learners. Users open a video with a subtitle file; the app can play normally or in a **sentence-by-sentence** mode driven entirely by subtitle timing — arrow-right jumps to the next subtitle line, space replays the current line's video span (repeatable), and a toggle reveals a translated-subtitle line alongside the original. If no subtitle file exists, the user can trigger subtitle generation.

## About the Design Files
The file in this bundle (`design_reference.dc.html`) is a **design reference built in HTML** — a static visual mockup showing intended look, layout, and states. It is NOT production code and should not be ported directly. Your task is to **recreate this design natively in Rust + Slint**, using `.slint` markup for layout/styling and Rust for state/logic, with `libmpv` for video playback and subtitle parsing/timing.

Open `design_reference.dc.html` in any browser to view it (it needs `support.js` alongside it, included in this folder). It contains two labeled sections:
- **id="1c"** — the main player screen
- **id="2a"** — two dialog states: "Open video" file picker, and "Open subtitles" panel (including the "no subtitles found → Generate subtitles" state)

## Fidelity
**High-fidelity.** Colors, spacing, type sizes, and copy in the mock are intentional — match them closely. Treat placeholder video frames (diagonal-stripe pattern) as a stand-in for the actual libmpv render surface.

## Screens / Views

### 1. Main Player (`#1c`)
**Purpose:** Watch a video with subtitles; switch between Normal and Sentence-by-sentence modes.

**Layout:** Single window, fixed vertical stack:
- Top bar (52px tall)
- Body (flex row, fills remaining height): video column (flex 1.5) + sentence panel column (flex 1, ~ equal width but narrower)
- Bottom hint bar (34px tall)

**Top bar** — background `#1c1c22`-ish (see tokens below), 1px bottom border, horizontal flex, space-between, 16px side padding:
- Left: 8px dot (accent color) + "TrangoPlayer" wordmark, 14px/600 Inter
- Center: segmented control, 2 items ("Normal" / "Sentence by sentence"), pill container with 3px padding, active segment filled with accent blue, inactive text is muted grey
- Right: two ghost buttons "Open video…" and "Open subtitles…", 1px border, monospace label, 12px

**Video column:**
- Video frame: rounded 8px, fills available space, margin 16px (less on the inner edge next to the sentence panel), diagonal stripe placeholder background, centered label text. A large circular play button (64px, translucent white fill, white left-pointing triangle) sits centered near the bottom of the frame — hover/pause state, not literal chrome.
- Scrub bar below: current time (mono, small, muted) — 4px track (rounded) with filled accent progress + white circular thumb — total time (mono, small, muted).

**Sentence panel (right column):**
- **Current sentence card:** rounded 8px, dark card background, border, ~20px padding. Header row: "Sentence 14 / 61" label (uppercase, mono, muted, 10px) left; "Translation" label + toggle switch right (pill switch, accent when on). Below: original-language sentence, 24px/600 Inter. Divider line. Translation sentence below in accent-tinted blue, 18px/500 — **hidden by default**, only shown when the toggle is on.
- **Sentence list card:** rounded 8px, fills remaining vertical space, scrollable. Header label "Sentence list" (uppercase, mono, muted, 10px). List of rows, each `index · sentence text…`; current row highlighted with a subtle accent-tinted background pill, others plain muted text. Row padding ~9px/10px, 6px radius.

**Bottom hint bar:** thin strip, centered row of keyboard hints separated by gap: "← previous sentence", "space · repeat sentence", "→ next sentence", "ctrl+t · toggle translation" (the first three only meaningful in sentence-by-sentence mode — in Normal mode this bar can be hidden or show standard playback shortcuts instead).

### 2. Open Video dialog (`#2a`, left mock)
**Purpose:** Pick a video file to open.
**Layout:** Modal centered over a dimmed (55% black) backdrop on top of the main player. Modal card: 640px wide, rounded 12px, dark surface, border, shadow.
- Header row: title "Open video file" + close "✕", bottom border.
- Scrollable file list: folder path label (mono, muted, uppercase), then rows — each row: small colored file-type chip + filename (13px/500) + subtitle line (duration · size, mono, muted). Selected/hovered row gets accent-tinted background.
- Footer: right-aligned "Cancel" (ghost) and "Open" (filled accent) buttons.

### 3. Open Subtitles dialog (`#2a`, right mock)
**Purpose:** Attach original-language subtitles and an optional translation; generate subtitles if none exist.
**Layout:** Same modal chrome as above, title reads "Subtitles for {video filename}".
- **Original language (DE) section:** label, then either a linked-file row (like the video list rows) if a subtitle is found, or — as shown — a dashed-border empty state row: "No subtitle file found" + a filled accent "Generate subtitles" button on the right.
- **Translation (EN) section:** label, linked-file row showing the translated `.srt` with a "linked" tag, plus helper text "or drop a translated .srt file here" (drag-and-drop target).
- Divider, then a small helper note: "Generating uses on-device speech recognition — no upload." (explains what the Generate action does; adjust to match your actual implementation, e.g. local Whisper model).
- Footer: "Cancel" (ghost) + "Done" (filled when valid, shown disabled/muted here since generation hasn't completed).

## Interactions & Behavior

**Mode switch:** Segmented control in the top bar toggles Normal ↔ Sentence-by-sentence. Persisted per session at minimum.

**Sentence-by-sentence mode** (core language-learning feature):
- Driven entirely by subtitle cue timing (start/end timestamps per line).
- **Right Arrow:** jump playhead to the start of the next subtitle cue and pause there — never autoplays (see `docs/src/specs/` for why: no mode starts playback on its own, only Space does).
- **Left Arrow:** jump to previous cue's start, same "always pause there" behavior as Right Arrow.
- **Space:** toggles playback of the current cue — if paused, plays from the cue's start through to its end and pauses there automatically; if already playing, pauses immediately instead (so you're never stuck waiting out a sentence you've heard enough of). Pressing Space again after it auto-paused replays the same span from the start every time — do not advance.
- **Translation toggle:** shows/hides the translated line under the original in the current-sentence card, via either the card's toggle switch or the **Ctrl+T** keyboard shortcut (works in both Normal and Sentence-by-sentence mode). Off by default. Purely visual — does not affect playback.
- **Sentence list:** clicking a row jumps to that cue (same behavior as arrow navigation) and highlights it as current.

**Normal mode:** standard continuous playback with scrub bar; subtitle panel can still show the current line (optionally hide the sentence-list card, or keep it as a chapter-like index — your call, mock only depicts sentence-by-sentence panel content).

**Open video:** opens native/file-picker-style modal, lists video files from a folder; selecting + "Open" loads it and attempts to auto-match a same-name subtitle file.

**Open subtitles:** opens modal scoped to the current video.
- If an original-language subtitle file matching the video is found on disk, show it as a linked row (same visual treatment as the translation row in the mock).
- If not found, show the empty/dashed state with "Generate subtitles" — this should kick off local subtitle generation (e.g., speech-to-text) and, on completion, replace the empty state with a linked-file row.
- Translation section always offers linking/dropping a second `.srt` in the viewer's native language.
- "Done" closes the modal and applies the linked files to the player.

## State Management
- `playbackMode`: `Normal | SentenceBySentence`
- `currentVideoPath`, `currentSubtitlePath` (original), `currentTranslationPath`
- `cues: Vec<{ index, start, end, text, translation }>` parsed from the subtitle file(s)
- `currentCueIndex`
- `showTranslation: bool` (default `false`)
- `subtitleGenerationStatus`: `Idle | Generating | Done | Error` (for the "Generate subtitles" flow)
- `isOpenVideoDialogOpen`, `isOpenSubtitlesDialogOpen`

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
