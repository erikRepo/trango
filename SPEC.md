# Handoff spec: views, interactions, state

This is the original functional handoff spec — split out of the root
`README.md` to keep that file short. See [STYLE.md](STYLE.md) for the
visual design reference and design tokens. For how the app actually
behaves today (including decisions made since this spec was written),
see the [docs/](https://erikrepo.github.io/trango/) mdBook, particularly
its Developer Guide → Design decisions page.

## Overview
A desktop video player for language learners. Users open a video with a subtitle file; the app can play normally or in a **sentence-by-sentence** mode driven entirely by subtitle timing — arrow-right jumps to the next subtitle line, space replays the current line's video span (repeatable), and a toggle reveals a translated-subtitle line alongside the original. If no subtitle file exists, the user can trigger subtitle generation.

## Screens / Views

### 1. Main Player (`#1c`)
**Purpose:** Watch a video with subtitles; switch between Normal and Sentence-by-sentence modes.

**Layout:** Single window, fixed vertical stack:
- Top bar (52px tall)
- Body (flex row, fills remaining height): video column (flex 1.5) + sentence panel column (flex 1, ~ equal width but narrower)
- Bottom hint bar (34px tall)

**Top bar** — background `#1c1c22`-ish (see tokens in `STYLE.md`), 1px bottom border, horizontal flex, space-between, 16px side padding:
- Left: 8px dot (accent color) + "TrangoPlayer" wordmark, 14px/600 Inter
- Center: segmented control, 2 items ("Normal" / "Sentence by sentence"), pill container with 3px padding, active segment filled with accent blue, inactive text is muted grey
- Right: two ghost buttons "Open video…" and "Subtitles…", 1px border, monospace label, 12px

**Video column:**
- Video frame: rounded 8px, fills available space, margin 16px (less on the inner edge next to the sentence panel), diagonal stripe placeholder background, centered label text. A large circular play button (64px, translucent white fill, white left-pointing triangle) sits centered near the bottom of the frame — hover/pause state, not literal chrome.
- Scrub bar below: current time (mono, small, muted) — 4px track (rounded) with filled accent progress + white circular thumb — total time (mono, small, muted).

**Sentence panel (right column):**
- **Current sentence card:** rounded 8px, dark card background, border, ~20px padding. Header row: "Sentence 14 / 61" label (uppercase, mono, muted, 10px) left; "Secondary subtitle" label + toggle switch right (pill switch, accent when on). Below: original-language sentence, 24px/600 Inter. Divider line. Translation sentence below in accent-tinted blue, 18px/500 — **hidden by default**, only shown when the toggle is on.
- **Sentence list card:** rounded 8px, fills remaining vertical space, scrollable. Header label "Sentence list" (uppercase, mono, muted, 10px). List of rows, each `index · sentence text…`; current row highlighted with a subtle accent-tinted background pill, others plain muted text. Row padding ~9px/10px, 6px radius.

**Bottom hint bar:** thin strip, centered row of keyboard hints separated by gap: "← previous sentence", "space · repeat sentence", "→ next sentence", "ctrl+t · toggle secondary subtitle" (the first three only meaningful in sentence-by-sentence mode — in Normal mode this bar can be hidden or show standard playback shortcuts instead).

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

**No mode autoplays.** Opening a video — with or without a subtitle file, CLI argument or Open Video dialog — always lands paused; only Space starts playback (see `docs/src/developer/specs.md`, "No mode autoplays"). In Sentence-by-sentence mode with cues loaded, it lands paused at the first cue's start rather than `0:00`.

**Space** is the only play/pause trigger, in both modes — pressing it while paused starts playback, pressing it again while playing pauses immediately:
- **Sentence-by-sentence mode, with a cue in focus:** plays that cue's span (seek to its start, play through to its end, pause there automatically) — this is the bounded "replay this sentence" behavior. Pressing Space again after it auto-paused replays the same span from the start every time — do not advance.
- **Normal mode, or Sentence-by-sentence with no subtitle linked yet:** plain, unbounded play/pause toggle — there's no single cue's span to bound playback to.

**Sentence-by-sentence mode** (core language-learning feature):
- Driven entirely by subtitle cue timing (start/end timestamps per line).
- **Right Arrow:** jump playhead to the start of the next subtitle cue and pause there — never autoplays (see `docs/src/developer/specs.md` for why: no mode starts playback on its own, only Space does).
- **Left Arrow:** jump to previous cue's start, same "always pause there" behavior as Right Arrow.
- **Secondary subtitle toggle:** shows/hides the translated line under the original in the current-sentence card, via either the card's toggle switch or the **Ctrl+T** keyboard shortcut (works in both Normal and Sentence-by-sentence mode). Off by default. Purely visual — does not affect playback.
- **Sentence list:** clicking a row jumps to that cue (same behavior as arrow navigation) and highlights it as current.

**Normal mode:** standard continuous playback with scrub bar; subtitle panel can still show the current line (optionally hide the sentence-list card, or keep it as a chapter-like index — your call, mock only depicts sentence-by-sentence panel content).

**Open video:** opens native/file-picker-style modal, lists video files from a folder; selecting + "Open" loads it and attempts to auto-match a same-name subtitle file.

**Open subtitles:** opens modal scoped to the current video.
- If an original-language subtitle file matching the video is found on disk, show it as a linked row (same visual treatment as the translation row in the mock).
- If not found, show the empty/dashed state with "Generate subtitles" — this should kick off local subtitle generation (e.g., speech-to-text) and, on completion, replace the empty state with a linked-file row.
- Secondary subtitle section always offers linking/dropping a second `.srt` in the viewer's native language.
- "Done" closes the modal and applies the linked files to the player.

## State Management
- `playbackMode`: `Normal | SentenceBySentence`
- `currentVideoPath`, `currentSubtitlePath` (original), `currentTranslationPath`
- `cues: Vec<{ index, start, end, text, translation }>` parsed from the subtitle file(s)
- `currentCueIndex`
- `showTranslation: bool` (default `false`)
- `subtitleGenerationStatus`: `Idle | Generating | Done | Error` (for the "Generate subtitles" flow)
- `isOpenVideoDialogOpen`, `isOpenSubtitlesDialogOpen`
