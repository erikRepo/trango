//! TrangoPlayer entry point.
//!
//! Initializes logging, then opens the Slint main window (see
//! `ui/app-window.slint`). libmpv integration and the rest of the UI are
//! wired in later development steps (see `TODO.md`).

mod config;
mod hebrew_word_merge;
mod model_picker;
mod niqud_model_picker;
mod niqud_pronunciation;
mod ollama_model_picker;
mod open_media_dialog;
mod open_subtitles_dialog;
mod sentence_card;
mod sentence_list;
mod settings_dialog;
mod subtitle_generation;
mod system_audio_capture;
mod video_player;
mod word_analysis;
mod word_timing_ui;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use playback_state::{MediaSource, PlaySpanCommand, PlaybackMode, PlayerState, SeekCommand};
use slint::Model;

slint::include_modules!();

/// Prints the current crate version to stdout.
fn print_version() {
    println!("trango {}", env!("CARGO_PKG_VERSION"));
}

/// Installs the `tracing` subscriber. `debug` (the `--debug` CLI flag,
/// see `extract_debug_flag`) is the primary way to turn on debug-level
/// logging — including `crates/word-analysis/src/ollama.rs`'s prompt/
/// response logging and `crates/niqud/src/onnx_client.rs`'s sentence/
/// result logging — without needing to export an environment variable;
/// when set it always wins, filtered to trango's own crates rather than
/// `debug`-level noise from every dependency (`winit` in particular is
/// very chatty). Without `--debug`, the `RUST_LOG` environment variable
/// still works as a lower-level escape hatch for finer-grained filtering
/// (e.g. `RUST_LOG=word_analysis=trace`), falling back to `info`-level
/// logging if that isn't set either — the same default
/// `tracing_subscriber::fmt::init()` used before either was wired in
/// explicitly (see `docs/src/developer/technology/tracing.md`).
fn init_logging(debug: bool) {
    let filter = if debug {
        tracing_subscriber::EnvFilter::new("info,trango=debug,word_analysis=debug,niqud=debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Reads the `--debug` CLI flag out of `args` (as `std::env::args()` would
/// give `main`), returning whether it was present and `args` with that
/// flag removed — so `video_path_from_args`/`subtitle_path_from_args`/
/// `translation_path_from_args`'s fixed positional indices (1/2/3) still
/// work regardless of where `--debug` was typed among them (e.g. `trango
/// --debug video.mp4 subs.srt` and `trango video.mp4 --debug subs.srt`
/// both load the same video/subtitle).
fn extract_debug_flag(args: Vec<String>) -> (bool, Vec<String>) {
    let debug = args.iter().any(|arg| arg == "--debug");
    let args = args.into_iter().filter(|arg| arg != "--debug").collect();
    (debug, args)
}

/// Converts `playback_state::PlaybackMode` to the Slint-generated
/// `PlaybackModeUi` mirrored into `AppWindow::playback-mode` — the two are
/// kept as separate types (rather than one shared enum) because
/// `slint::include_modules!()` generates `PlaybackModeUi` into this same
/// module, and naming it `PlaybackMode` too would collide with the
/// `playback_state` import above.
fn to_ui_mode(mode: PlaybackMode) -> PlaybackModeUi {
    match mode {
        PlaybackMode::Normal => PlaybackModeUi::Normal,
        PlaybackMode::SentenceBySentence => PlaybackModeUi::SentenceBySentence,
    }
}

/// The inverse of [`to_ui_mode`], converting the Normal/Sentence-by-sentence
/// segmented control's clicked segment back to the `playback_state` mode it
/// names.
fn from_ui_mode(mode: PlaybackModeUi) -> PlaybackMode {
    match mode {
        PlaybackModeUi::Normal => PlaybackMode::Normal,
        PlaybackModeUi::SentenceBySentence => PlaybackMode::SentenceBySentence,
    }
}

/// Converts `playback_state::MediaSource` to the Slint-generated
/// `MediaSourceUi` mirrored into `AppWindow::media-source` — kept as a
/// separate type from `MediaSource` for the same reason `to_ui_mode` keeps
/// `PlaybackModeUi` separate from `PlaybackMode`.
fn to_ui_source(source: MediaSource) -> MediaSourceUi {
    match source {
        MediaSource::Video => MediaSourceUi::Video,
        MediaSource::Audio => MediaSourceUi::Audio,
    }
}

/// The inverse of [`to_ui_source`], converting the Video/Audio segmented
/// control's clicked segment back to the `playback_state` source it names.
fn from_ui_source(source: MediaSourceUi) -> MediaSource {
    match source {
        MediaSourceUi::Video => MediaSource::Video,
        MediaSourceUi::Audio => MediaSource::Audio,
    }
}

/// Owns a fresh `PlayerState` — defaulting to `SentenceBySentence` mode and
/// `Video` source, the primary language-learning use case — and mirrors
/// those defaults into the window's `playback-mode`/`media-source`
/// properties, since `app-window.slint` itself only hardcodes `Normal`/
/// `Video`. Also wires the window's `select-mode` callback (invoked by the
/// top bar's Normal/Sentence-by-sentence segmented control with its clicked
/// segment's target mode) to `PlayerState::set_mode()`, the window's
/// `select-media-source` callback (invoked by the top bar's Video/Audio
/// segmented control) to `PlayerState::set_media_source()`, mirroring each
/// subsequent change back into `playback-mode`/`media-source` respectively,
/// and the window's `toggle-translation` callback (invoked by the current-
/// sentence card's toggle switch) to `PlayerState::toggle_translation()`,
/// mirroring `show_translation` into the window's `show-translation`
/// property the same way. Returns the shared state so callers can inspect
/// it (used by tests; later steps will read it too).
fn wire_player_state(window: &AppWindow) -> Rc<RefCell<PlayerState>> {
    let state = Rc::new(RefCell::new(PlayerState::new()));
    window.set_playback_mode(to_ui_mode(state.borrow().mode));
    window.set_media_source(to_ui_source(state.borrow().media_source));
    window.set_show_translation(state.borrow().show_translation);

    let state_for_callback = Rc::clone(&state);
    let window_weak = window.as_weak();
    window.on_select_mode(move |ui_mode| {
        let mode = from_ui_mode(ui_mode);
        state_for_callback.borrow_mut().set_mode(mode);
        tracing::debug!(?mode, "playback mode selected");
        if let Some(window) = window_weak.upgrade() {
            window.set_playback_mode(to_ui_mode(mode));
        }
    });

    let source_state = Rc::clone(&state);
    let source_window_weak = window.as_weak();
    window.on_select_media_source(move |ui_source| {
        let source = from_ui_source(ui_source);
        source_state.borrow_mut().set_media_source(source);
        tracing::debug!(?source, "media source selected");
        if let Some(window) = source_window_weak.upgrade() {
            window.set_media_source(to_ui_source(source));
            // Re-derives the sentence card/list display for the panel just
            // switched to: blanked (an empty PlayerState, the same
            // placeholder shown before anything's ever loaded) if it's the
            // Audio source and nothing loaded there yet matches, otherwise
            // the real state — restoring it when switching back to a
            // source that's ready. See panel_content_ready's doc comment.
            let display_state = if panel_content_ready(&window) {
                source_state.borrow().clone()
            } else {
                PlayerState::new()
            };
            sentence_card::update_sentence_card(&window, &display_state);
            sentence_list::update_sentence_list(&window, &display_state);
        }
    });

    let translation_state = Rc::clone(&state);
    let translation_window_weak = window.as_weak();
    window.on_toggle_translation(move || {
        let show_translation = {
            let mut state = translation_state.borrow_mut();
            state.toggle_translation();
            state.show_translation
        };
        tracing::debug!(show_translation, "translation visibility toggled");
        if let Some(window) = translation_window_weak.upgrade() {
            window.set_show_translation(show_translation);
        }
    });

    state
}

/// Wires the window's `next-cue`, `previous-cue`, `repeat-cue`, and
/// `jump-to-cue` callbacks — invoked by `app-window.slint`'s `key-pressed`
/// handler for Right/Left (`SentenceBySentence` mode only) and Space
/// (both modes), and by the sentence list's row clicks, respectively.
///
/// `next-cue`/`previous-cue`/`jump-to-cue` land on a different cue's start
/// and always leave mpv paused there — no mode autoplays on navigation
/// alone (see `docs/src/developer/specs.md`) — via `PlayerState`'s matching methods,
/// mirroring the result into the sentence card/list and handing the
/// produced `SeekCommand` to `video_player::VideoPlayer::seek_and_pause`.
///
/// `repeat-cue` (Space) doesn't move the cursor, so it skips the sentence
/// card/list refresh entirely. If a cue is currently in focus
/// (`PlayerState::repeat_current_cue` returns `Some`), it hands that
/// `PlaySpanCommand` to `video_player::VideoPlayer::toggle_play_span`,
/// which plays/replays that cue's bounded span. Otherwise — `Normal` mode,
/// or `SentenceBySentence` mode before any subtitle is linked, where no
/// single cue's span is the relevant unit — it falls back to
/// `video_player::VideoPlayer::toggle_playback`, a plain unbounded
/// play/pause toggle.
fn wire_cue_navigation(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    video_player: Rc<video_player::VideoPlayer>,
) {
    window.on_next_cue(cue_navigation_handler(
        window,
        state,
        &video_player,
        PlayerState::next_cue,
    ));
    window.on_previous_cue(cue_navigation_handler(
        window,
        state,
        &video_player,
        PlayerState::previous_cue,
    ));

    let repeat_state = Rc::clone(state);
    let repeat_video_player = Rc::clone(&video_player);
    window.on_repeat_cue(move || match repeat_state.borrow().repeat_current_cue() {
        Some(command) => repeat_video_player.toggle_play_span(command),
        None => repeat_video_player.toggle_playback(),
    });

    let jump_state = Rc::clone(state);
    let jump_window_weak = window.as_weak();
    let jump_video_player = Rc::clone(&video_player);
    window.on_jump_to_cue(move |index| {
        let Ok(index) = usize::try_from(index) else {
            tracing::warn!(index, "ignoring negative sentence list row index");
            return;
        };
        let command = jump_state.borrow_mut().jump_to_cue(index);
        if let Some(window) = jump_window_weak.upgrade() {
            apply_navigation_result(&window, &jump_state.borrow(), &jump_video_player, command);
        }
    });
}

/// Wires the window's `seek-requested` callback — invoked by the scrub bar
/// (`app-window.slint`'s `ScrubBar`) on click/drag with the pointer's
/// fraction across the track — to `video_player::VideoPlayer::seek_to_fraction`.
/// Unlike `wire_cue_navigation`'s callbacks, this never touches play/pause
/// state or the sentence card/list: dragging the scrub bar only relocates
/// the playhead within whichever mode is active.
fn wire_scrub_bar(window: &AppWindow, video_player: Rc<video_player::VideoPlayer>) {
    window.on_seek_requested(move |fraction| video_player.seek_to_fraction(fraction));
}

/// Wires the window's `pause-playback` callback — invoked by the top bar's
/// Video/Audio segmented control (`app-window.slint`) right before
/// `select-media-source` on every click — to
/// `video_player::VideoPlayer::pause()`. Both sources share the same mpv
/// instance/loaded file (see `AppWindow::loaded-media-source`'s doc
/// comment), so without this, switching away from whichever panel is
/// currently playing would leave it running audibly behind the other one.
fn wire_pause_playback(window: &AppWindow, video_player: Rc<video_player::VideoPlayer>) {
    window.on_pause_playback(move || video_player.pause());
}

/// Wires the window's `speed-requested` callback — invoked by the always-
/// visible playback-speed slider (`app-window.slint`'s `SpeedSlider`) on
/// click/drag with the pointer's fraction across the track. Maps that
/// fraction to an actual mpv speed with `playback_state::speed_from_fraction`
/// (max is normal speed — see `SpeedSlider`'s doc comment), applies it via
/// `video_player::VideoPlayer::set_speed`, and mirrors the result back into
/// `current-playback-speed`/`-label` so the slider's thumb and value text
/// reflect it. Also sets the window's initial "1.00x" state, matching
/// `AppWindow`'s own property defaults.
fn wire_speed_slider(window: &AppWindow, video_player: Rc<video_player::VideoPlayer>) {
    window.set_current_playback_speed(playback_state::MAX_SPEED as f32);
    window.set_current_playback_speed_label(
        playback_state::format_speed_label(playback_state::MAX_SPEED).into(),
    );

    let window_weak = window.as_weak();
    window.on_speed_requested(move |fraction| {
        let speed = playback_state::speed_from_fraction(fraction);
        video_player.set_speed(speed);
        if let Some(window) = window_weak.upgrade() {
            window.set_current_playback_speed(speed as f32);
            window
                .set_current_playback_speed_label(playback_state::format_speed_label(speed).into());
        }
    });
}

/// Builds the closure behind one `wire_cue_navigation` key-driven callback:
/// runs `navigate` against the shared `PlayerState`, then applies the result
/// the same way the sentence list's row-click handler does (see
/// `apply_navigation_result`).
fn cue_navigation_handler(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    video_player: &Rc<video_player::VideoPlayer>,
    navigate: impl Fn(&mut PlayerState) -> Option<SeekCommand> + 'static,
) -> impl FnMut() + 'static {
    let state = Rc::clone(state);
    let window_weak = window.as_weak();
    let video_player = Rc::clone(video_player);
    move || {
        let command = navigate(&mut state.borrow_mut());
        if let Some(window) = window_weak.upgrade() {
            apply_navigation_result(&window, &state.borrow(), &video_player, command);
        }
    }
}

/// Mirrors a navigation result into the sentence card and sentence list, and
/// — if a `SeekCommand` was produced — hands it to `video_player` to drive
/// mpv. Shared by arrow-key handling and the sentence list's row-click
/// handling so both paths behave identically, per SPEC.md's "Sentence list"
/// spec ("same behavior as arrow navigation").
fn apply_navigation_result(
    window: &AppWindow,
    state: &PlayerState,
    video_player: &video_player::VideoPlayer,
    command: Option<SeekCommand>,
) {
    sentence_card::update_sentence_card(window, state);
    sentence_list::update_sentence_list(window, state);
    if let Some(command) = command {
        video_player.seek_and_pause(command);
    }
}

/// Reads the video path to play (if any) from CLI arguments, as used by
/// `main`. `args` is expected to include the program name at index 0 (i.e.
/// `std::env::args()`), matching Vaihe 11's `trango <path/to/video>` usage.
/// A video can also be picked in-app via the top bar's "Open…" button
/// (see `wire_open_media_dialog`) instead of a CLI argument.
fn video_path_from_args(args: &[String]) -> Option<PathBuf> {
    args.get(1).map(PathBuf::from)
}

/// Reads the subtitle path to load (if any) from CLI arguments, as used by
/// `main`. `args` is expected to include the program name at index 0,
/// matching Vaihe 14's `trango <path/to/video> <path/to/subs.srt>` usage —
/// the Open Subtitles dialog for picking a file in-app arrives in a later
/// step.
fn subtitle_path_from_args(args: &[String]) -> Option<PathBuf> {
    args.get(2).map(PathBuf::from)
}

/// Reads the translation subtitle path to merge in (if any) from CLI
/// arguments, as used by `main`. `args` is expected to include the program
/// name at index 0, matching Vaihe 17's
/// `trango <path/to/video> <path/to/subs.srt> <path/to/subs.en.srt>` usage.
fn translation_path_from_args(args: &[String]) -> Option<PathBuf> {
    args.get(3).map(PathBuf::from)
}

/// Reads and parses `path` into cues. Logs and returns `None` if the file
/// can't be read or doesn't parse, so a bad path doesn't panic the caller.
fn parse_subtitle_file(path: &Path) -> Option<Vec<subtitle::Cue>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::error!(?path, %err, "failed to read subtitle file");
            return None;
        }
    };
    match subtitle::parse_srt(&contents) {
        Ok(cues) => Some(cues),
        Err(err) => {
            tracing::error!(?path, %err, "failed to parse subtitle file");
            None
        }
    }
}

/// Reads and parses `subtitle_path` into cues, merges in `translation_path`'s
/// cues if given (via `subtitle::merge_translation`), loads the result into
/// `state`, and mirrors the resulting current cue into the window's
/// sentence card and sentence list. Leaves `state` untouched and returns
/// `false` if `subtitle_path` can't be read or parsed, since a bad
/// subtitle path shouldn't prevent the video from playing — callers use
/// the return value to decide whether to record `subtitle_path` as the
/// video's linked original subtitle (see `main`'s and
/// `open_selected_media`'s `CurrentMedia` tracking). A `translation_path`
/// that can't be read or parsed is logged and simply skipped — the
/// original cues still load, just without translations, and `true` is
/// still returned since the original subtitle itself loaded fine.
fn load_subtitles(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    subtitle_path: &Path,
    translation_path: Option<&Path>,
) -> bool {
    let Some(mut cues) = parse_subtitle_file(subtitle_path) else {
        return false;
    };
    if let Some(translation_path) = translation_path {
        if let Some(translation_cues) = parse_subtitle_file(translation_path) {
            cues = subtitle::merge_translation(cues, translation_cues);
        }
    }
    tracing::info!(?subtitle_path, cue_count = cues.len(), "loaded subtitles");
    state.borrow_mut().set_cues(cues);
    sentence_card::update_sentence_card(window, &state.borrow());
    sentence_list::update_sentence_list(window, &state.borrow());
    true
}

/// Tracks the paths behind the currently open media/subtitle/translation.
/// Nothing here drives playback directly (that's `PlayerState`/
/// `video_player::VideoPlayer`) — it exists so the Open Subtitles dialog
/// (Vaihe 19) knows what media it's scoped to and what's already linked,
/// without re-deriving it from disk on every open. That matters because a
/// CLI-loaded subtitle (`trango video.mp4 custom-name.srt`) may not share
/// the media's filename stem, unlike an auto-matched one
/// (`open_media_dialog::matching_subtitle_path`) — so "what's linked" isn't
/// always re-derivable by searching disk alone. `media_path` holds a video
/// path in the Video source, or an opened/recorded `.wav` path in the Audio
/// source (`TODO.md` Vaihe 28) — both load through the same
/// `video_player::VideoPlayer::load_video`.
#[derive(Debug, Clone, Default)]
struct CurrentMedia {
    /// The currently open video or audio file's path, if any.
    media_path: Option<PathBuf>,
    /// The currently linked original-language subtitle's path, if any.
    subtitle_path: Option<PathBuf>,
    /// The currently linked translation subtitle's path, if any.
    translation_path: Option<PathBuf>,
}

/// Resolves the folder the Open dialog lists by default in `MediaKind::Video`:
/// the CLI video path's parent directory if one was given (Vaihe 11's
/// `trango <path/to/video>` usage — likely where the user keeps other
/// videos too), otherwise `config`'s last-opened video folder
/// (`TrangoConfig::video_folder`, kept up to date by `open_selected_media`),
/// otherwise the current working directory. An in-dialog folder switcher is
/// out of scope for Vaihe 18 — see `docs/src/developer/specs.md`.
fn default_video_folder(args: &[String], config: &config::TrangoConfig) -> PathBuf {
    video_path_from_args(args)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .filter(|parent| !parent.as_os_str().is_empty())
        .or_else(|| config.video_folder.clone())
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|err| {
                tracing::warn!(%err, "failed to read current directory; falling back to \".\"");
                PathBuf::from(".")
            })
        })
}

/// The `MediaKind` the Open dialog should list for `source` — `Video` for
/// the Video source's "Open…" button, `Audio` for the Audio source's
/// (`TODO.md` Vaihe 28: the two sources share one button and one dialog,
/// distinguished by whichever source is currently active).
fn media_kind_for_source(source: MediaSource) -> open_media_dialog::MediaKind {
    match source {
        MediaSource::Video => open_media_dialog::MediaKind::Video,
        MediaSource::Audio => open_media_dialog::MediaKind::Audio,
    }
}

/// Whether the visible panel's own sentence content should be shown —
/// mirrors `app-window.slint`'s `ScrubBar`/`SpeedSlider` gating (`media-
/// source != Audio || media-ready`): the Video source's is always ready,
/// the Audio source's only once its loaded file's kind actually matches.
/// Used by `wire_player_state`'s `on_select_media_source` handler to blank
/// the current-sentence card/sentence list when switching to a not-yet-
/// loaded Audio panel, rather than leaving the previous source's sentence
/// stuck on screen, and by `wire_word_analysis_popup` so Ctrl+A doesn't
/// analyze a sentence that isn't actually being shown.
fn panel_content_ready(window: &AppWindow) -> bool {
    window.get_media_source() != MediaSourceUi::Audio || window.get_media_ready()
}

/// The `MediaSourceUi` panel that shows a file of `kind` once loaded —
/// mirrors `media_kind_for_source`'s mapping. `open_selected_media` sets
/// `AppWindow::loaded-media-source` to this right after loading, so
/// `app-window.slint`'s `media-ready` can tell a video loaded from before a
/// source switch apart from a matching file for the panel currently shown.
fn ui_source_for_media_kind(kind: open_media_dialog::MediaKind) -> MediaSourceUi {
    match kind {
        open_media_dialog::MediaKind::Video => MediaSourceUi::Video,
        open_media_dialog::MediaKind::Audio => MediaSourceUi::Audio,
    }
}

/// The Open dialog's title for `kind`, shown in `FileListDialog`'s header.
fn open_media_dialog_title(kind: open_media_dialog::MediaKind) -> &'static str {
    match kind {
        open_media_dialog::MediaKind::Video => "Open video file",
        open_media_dialog::MediaKind::Audio => "Open audio file",
    }
}

/// Wires the top bar's single "Open…" button (Vaihe 18, generalized to
/// audio in Vaihe 28): `open-media-dialog-requested` reads `state`'s current
/// `MediaSource` to decide which kind of file to list
/// (`media_kind_for_source`) — `Video` lists `video_default_folder`,
/// `Audio` lists `system_audio_capture::default_recording_folder` off a
/// freshly loaded config (the last folder a recording was opened from or
/// written to) — and opens the modal (`open_media_dialog::open_dialog`).
/// `select-open-media-row` either navigates to a different folder (an
/// `Up`/`Folder` row — re-listing in place, keeping the same `MediaKind`) or
/// marks a file row selected (`open_media_dialog::mark_selected`);
/// `confirm-open-media` loads the selected file (see `open_selected_media`);
/// `cancel-open-media-dialog` (backdrop/✕/Cancel) just closes it.
fn wire_open_media_dialog(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    video_player: Rc<video_player::VideoPlayer>,
    video_default_folder: PathBuf,
    current_media: Rc<RefCell<CurrentMedia>>,
) {
    let entries: Rc<RefCell<Vec<open_media_dialog::FolderEntry>>> =
        Rc::new(RefCell::new(Vec::new()));
    let kind: Rc<RefCell<open_media_dialog::MediaKind>> =
        Rc::new(RefCell::new(open_media_dialog::MediaKind::Video));

    let request_window_weak = window.as_weak();
    let request_entries = Rc::clone(&entries);
    let request_kind = Rc::clone(&kind);
    let request_state = Rc::clone(state);
    window.on_open_media_dialog_requested(move || {
        let Some(window) = request_window_weak.upgrade() else {
            return;
        };
        let media_kind = media_kind_for_source(request_state.borrow().media_source);
        let folder = match media_kind {
            open_media_dialog::MediaKind::Video => video_default_folder.clone(),
            open_media_dialog::MediaKind::Audio => {
                system_audio_capture::default_recording_folder(&config::load())
            }
        };
        window.set_open_media_dialog_title(open_media_dialog_title(media_kind).into());
        let files = open_media_dialog::list_folder_entries(&folder, media_kind);
        open_media_dialog::open_dialog(&window, &folder, &files);
        *request_entries.borrow_mut() = files;
        *request_kind.borrow_mut() = media_kind;
    });

    let cancel_window_weak = window.as_weak();
    window.on_cancel_open_media_dialog(move || {
        if let Some(window) = cancel_window_weak.upgrade() {
            window.set_is_open_media_dialog_open(false);
        }
    });

    let select_window_weak = window.as_weak();
    let select_entries = Rc::clone(&entries);
    let select_kind = Rc::clone(&kind);
    window.on_select_open_media_row(move |index| {
        let Some(window) = select_window_weak.upgrade() else {
            return;
        };
        let target_folder = usize::try_from(index).ok().and_then(|index| {
            match select_entries.borrow().get(index)? {
                open_media_dialog::FolderEntry::Up(path)
                | open_media_dialog::FolderEntry::Folder { path, .. } => Some(path.clone()),
                open_media_dialog::FolderEntry::File(_) => None,
            }
        });
        if let Some(target_folder) = target_folder {
            let files =
                open_media_dialog::list_folder_entries(&target_folder, *select_kind.borrow());
            open_media_dialog::open_dialog(&window, &target_folder, &files);
            *select_entries.borrow_mut() = files;
            return;
        }
        window.set_open_media_selected_index(index);
        open_media_dialog::mark_selected(&window, &select_entries.borrow(), index);
    });

    let confirm_window_weak = window.as_weak();
    let confirm_entries = Rc::clone(&entries);
    let confirm_kind = Rc::clone(&kind);
    let confirm_state = Rc::clone(state);
    let confirm_media = Rc::clone(&current_media);
    window.on_confirm_open_media(move || {
        let Some(window) = confirm_window_weak.upgrade() else {
            return;
        };
        let media_path = usize::try_from(window.get_open_media_selected_index())
            .ok()
            .and_then(|index| confirm_entries.borrow().get(index).cloned())
            .and_then(|entry| match entry {
                open_media_dialog::FolderEntry::File(file) => Some(file.path),
                _ => None,
            });
        let Some(media_path) = media_path else {
            return;
        };
        window.set_is_open_media_dialog_open(false);
        open_selected_media(
            &window,
            &confirm_state,
            &video_player,
            *confirm_kind.borrow(),
            &media_path,
            &confirm_media,
        );
    });
}

/// Loads `media_path` into `video_player` — always the same already-attached
/// `VideoPlayer` (see `video_player::VideoPlayer::attach`'s doc comment for
/// why it's attached once, unconditionally, at startup rather than lazily
/// here); mpv plays a `.wav` file the same way it plays a video, just with
/// no picture (`TODO.md` Vaihe 28). Resolves a same-stem `.srt` first
/// (`open_media_dialog::matching_subtitle_path`) and loads it via
/// `load_subtitles` if found, clearing any previously loaded cues
/// otherwise — done before `load_video` so that, in `SentenceBySentence`
/// mode, the start-of-playback pause lands on the new file's first cue
/// rather than a stale one from whatever was open before. Called by
/// `wire_open_media_dialog`'s `confirm-open-media` handler, and by
/// `system_audio_capture`'s stop handler once a fresh recording finishes
/// writing, so it lands in the player the same way an explicitly opened
/// file does. Also resets `current_media` to the new file (clearing any
/// translation link from whatever was open before), so the Open Subtitles
/// dialog (Vaihe 19) reflects it the next time it's shown. Persists
/// `media_path`'s parent folder to `config::TrangoConfig::video_folder` or
/// `audio_recording_folder` depending on `kind` (`config::save`), so the
/// Open dialog defaults to wherever the user last opened a file of that
/// kind from, on the next run. Finally mirrors `kind` into
/// `AppWindow::loaded-media-source` (`ui_source_for_media_kind`) so
/// `app-window.slint`'s `media-ready` can gate playback controls to the
/// panel the loaded file actually matches.
fn open_selected_media(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    video_player: &video_player::VideoPlayer,
    kind: open_media_dialog::MediaKind,
    media_path: &Path,
    current_media: &Rc<RefCell<CurrentMedia>>,
) {
    if let Some(folder) = media_path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        let mut config = config::load();
        match kind {
            open_media_dialog::MediaKind::Video => config.video_folder = Some(folder.to_path_buf()),
            open_media_dialog::MediaKind::Audio => {
                config.audio_recording_folder = Some(folder.to_path_buf())
            }
        }
        config::save(&config);
    }

    let subtitle_path = open_media_dialog::matching_subtitle_path(media_path);
    let mut subtitle_loaded = false;
    if let Some(subtitle_path) = &subtitle_path {
        tracing::info!(
            ?subtitle_path,
            "auto-matched subtitle file for opened media"
        );
        subtitle_loaded = load_subtitles(window, state, subtitle_path, None);
    }
    if !subtitle_loaded {
        state.borrow_mut().set_cues(Vec::new());
        sentence_card::update_sentence_card(window, &state.borrow());
        sentence_list::update_sentence_list(window, &state.borrow());
    }

    *current_media.borrow_mut() = CurrentMedia {
        media_path: Some(media_path.to_path_buf()),
        subtitle_path: subtitle_loaded.then_some(subtitle_path).flatten(),
        translation_path: None,
    };

    video_player.load_video(window, media_path, &state.borrow());
    window.set_loaded_media_source(ui_source_for_media_kind(kind));
}

/// Wires the Open Subtitles dialog (Vaihe 19): the top bar's
/// `open-subtitles-dialog-requested` callback resolves the current video's
/// original-language subtitle — `current_media`'s tracked path, falling
/// back to a same-stem `.srt` search (`open_media_dialog::matching_subtitle_path`)
/// for the case a CLI-loaded subtitle didn't get tracked with a matching
/// name — and opens the modal (`open_subtitles_dialog::open_dialog`).
/// `cancel`/`confirm-open-subtitles-dialog` (backdrop/✕/Cancel/Done) just
/// close it: both sections are already live the moment they're linked, not
/// deferred to "Done" (see `AppWindow`'s doc comment on those callbacks).
/// `generate-subtitles-requested` (`TODO.md` Vaihe 20/21.5/21.6) runs
/// `subtitle::WhisperCliGenerator` — configured by `whisper_cli_generator`
/// with `selected_model`'s currently picked model (see
/// `wire_model_picker`) and its language flag
/// (`model_picker::language_flag`) — on a background thread via
/// `subtitle_generation::spawn_generate` — real transcription can take
/// seconds to minutes, so running it on the UI thread would freeze the
/// whole app. Its result is posted back to the UI thread with
/// `slint::invoke_from_event_loop` (same pattern as `video_player.rs`'s
/// `load_file`) and applied with `subtitle_generation::apply_result`, which
/// mirrors `Idle -> Generating -> Done`/`Error` into
/// `subtitle-generation-status`/`-error-message` and, on success, the
/// dialog's original row. That callback may only carry `Send` data across
/// the thread boundary (a `Weak<AppWindow>` and the owned `Result`), so on
/// success it hands off to `AppWindow::subtitle-generated` — invoked with
/// just the new path, handled by a separate closure set up here that does
/// hold the `Rc<RefCell<PlayerState>>`/`Rc<RefCell<CurrentMedia>>` needed to
/// load the subtitle into the player and record it in `current_media`, same
/// as a picked translation is below. No model selected yet is a clear
/// `Error` rather than an attempted run — the button is also disabled in
/// that state (`AppWindow::whisper-model-selected`), this is the defensive
/// fallback. `reload_video` is then called with the video's path — a
/// generation run can take long enough (seconds to minutes) that a short
/// video left playing may have already reached EOF and idled mpv's core by
/// the time it finishes, which fails any subsequent cue-navigation seek
/// outright (mpv error `Raw(-12)`, see `video_player.rs`'s
/// `apply_pending_start_seek` doc comment); reloading the video the same
/// way opening it fresh does recovers a normal, seekable state. Taking a
/// plain closure here rather than a `Rc<video_player::VideoPlayer>`
/// directly keeps this function testable without a real mpv render
/// context, which `VideoPlayer::attach` needs and this module's tests
/// don't have (see `main`'s caller for the real
/// `VideoPlayer::load_video`-backed closure).
///
/// `link-translation-requested` opens a nested `FileListDialog` picker
/// (`open_subtitles_dialog::open_translation_picker`) over the video's
/// folder's `.srt` files — real OS drag-and-drop isn't available with
/// Slint 1.17.1's winit backend, see `open_subtitles_dialog`'s module doc.
/// Picking a row there and confirming re-merges cues with the new
/// translation via `load_subtitles`, updates `current_media`, and mirrors
/// the pick back into the (still-open) Open Subtitles dialog's translation
/// row (`open_subtitles_dialog::mark_translation_linked`).
///
/// `select-whisper-model-requested` opens a second nested `FileListDialog`,
/// wired by `wire_model_picker` — see its doc comment.
///
/// Builds a `subtitle::WhisperCliGenerator` for `model_path` (see
/// `wire_model_picker`'s doc comment for why the model itself isn't read
/// from an environment variable, unlike the binary paths below) and its
/// inferred language (`model_picker::language_flag`).
///
/// - `TRANGO_WHISPER_CLI_PATH` env var: path or bare name of the
///   `whisper-cli` binary. Defaults to `"whisper-cli"`, resolved via
///   `PATH` — see `docs/src/usage` for installing it.
/// - `TRANGO_FFMPEG_PATH` env var: path or bare name of the `ffmpeg`
///   binary `WhisperCliGenerator` uses to extract audio before handing it
///   to `whisper-cli` (which can't read most video containers directly —
///   see `WhisperCliGenerator`'s doc comment). Defaults to `"ffmpeg"`,
///   resolved via `PATH`.
pub(crate) fn whisper_cli_generator(model_path: PathBuf) -> subtitle::WhisperCliGenerator {
    let binary_path = std::env::var_os("TRANGO_WHISPER_CLI_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("whisper-cli"));
    let ffmpeg_path = ffmpeg_path_from_env();
    let language = model_picker::language_flag(&model_path).to_string();
    tracing::info!(
        ?binary_path,
        ?ffmpeg_path,
        ?model_path,
        %language,
        "configured whisper-cli generator"
    );
    subtitle::WhisperCliGenerator {
        binary_path,
        ffmpeg_path,
        model_path: Some(model_path),
        language: Some(language),
    }
}

/// Builds a `subtitle::WhisperCliWordSegmenter` for the Ctrl+W word-timing
/// popup (`TODO.md` Vaihe 32), mirroring [`whisper_cli_generator`] (same
/// `TRANGO_WHISPER_CLI_PATH`/`TRANGO_FFMPEG_PATH`/`language_flag`
/// derivation from the same selected whisper model) plus one addition:
/// `dtw_preset`, derived from `model_path`'s filename via
/// `subtitle::dtw_preset_for_model` — `None` for an unrecognized model
/// name, which makes the segmenter omit `-dtw` rather than guess wrong.
pub(crate) fn whisper_cli_word_segmenter(model_path: PathBuf) -> subtitle::WhisperCliWordSegmenter {
    let binary_path = std::env::var_os("TRANGO_WHISPER_CLI_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("whisper-cli"));
    let ffmpeg_path = ffmpeg_path_from_env();
    let language = model_picker::language_flag(&model_path).to_string();
    let dtw_preset = subtitle::dtw_preset_for_model(&model_path).map(str::to_string);
    tracing::info!(
        ?binary_path,
        ?ffmpeg_path,
        ?model_path,
        %language,
        ?dtw_preset,
        "configured whisper-cli word segmenter"
    );
    subtitle::WhisperCliWordSegmenter {
        binary_path,
        ffmpeg_path,
        model_path: Some(model_path),
        language: Some(language),
        dtw_preset,
    }
}

/// The `TRANGO_FFMPEG_PATH` env var (path or bare name of the `ffmpeg`
/// binary), defaulting to `"ffmpeg"` resolved via `PATH` — shared by
/// `whisper_cli_generator` (audio extraction ahead of `whisper-cli`) and
/// `main`'s `system_audio_capture::wire_audio_capture` call (system audio
/// capture, `TODO.md` Vaihe 26), so ffmpeg's location is configured once
/// for both.
fn ffmpeg_path_from_env() -> PathBuf {
    std::env::var_os("TRANGO_FFMPEG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

/// How long [`niqud_client_from_config`] waits for `OnnxNiqudClient::load`
/// before giving up — real loads finish in well under a second, but a
/// found-yet-incompatible `libonnxruntime.so` has been observed to hang
/// `ort`'s session setup indefinitely rather than erroring cleanly (see
/// `docs/src/developer/technology/ort.md`), and this call runs
/// synchronously at startup, before the window is even shown.
const NIQUD_LOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Builds the niqud client used by both `wire_word_analysis_batch` and
/// `wire_word_analysis_popup` to derive Hebrew pronunciation guides (see
/// `docs/src/developer/specs.md`'s "Hebrew pronunciation" entry) from
/// `config.niqud_model_path` (a `.onnx` model file, with `tokenizer.json`
/// expected as a sibling — set via the Settings dialog's "Hebrew niqud
/// model" row). Called once at startup, not per word-analysis call —
/// `niqud::OnnxNiqudClient` is `Clone` specifically so the loaded
/// model/session can be reused for the life of the process rather than
/// reloaded every time; changing the path in Settings takes effect on the
/// next restart, not live.
///
/// Runs the actual load on a background thread with a bounded wait
/// (`NIQUD_LOAD_TIMEOUT`) rather than calling it directly, so a hang
/// inside `ort` can't freeze trango's startup — a timed-out load leaves
/// that thread blocked indefinitely in the background rather than
/// crashing or hanging the app, an acceptable tradeoff since it's a
/// single extra idle thread, not a resource that grows unbounded.
///
/// Returns `None` (logging a warning, not an error — this is an expected,
/// supported state, not a failure) if no path is configured, loading
/// fails, or it times out; word analysis then falls back to Ollama's own
/// pronunciation guess rather than failing outright
/// (`niqud_pronunciation::apply_niqud_pronunciation`).
fn niqud_client_from_config(config: &config::TrangoConfig) -> Option<niqud::OnnxNiqudClient> {
    let model_path = config.niqud_model_path.clone()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(niqud::OnnxNiqudClient::load(&model_path));
    });
    match rx.recv_timeout(NIQUD_LOAD_TIMEOUT) {
        Ok(Ok(client)) => Some(client),
        Ok(Err(err)) => {
            tracing::warn!(%err, "failed to load niqud model, Hebrew word analysis will fall back to Ollama's own pronunciation guess");
            None
        }
        Err(_timed_out) => {
            tracing::warn!(
                timeout_secs = NIQUD_LOAD_TIMEOUT.as_secs(),
                "niqud model loading timed out (a hung/incompatible onnxruntime install?), \
                 Hebrew word analysis will fall back to Ollama's own pronunciation guess"
            );
            None
        }
    }
}

fn wire_open_subtitles_dialog(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    reload_video: impl Fn(&AppWindow, &Path, &PlayerState) + 'static,
    current_media: Rc<RefCell<CurrentMedia>>,
    selected_model: Rc<RefCell<Option<PathBuf>>>,
) {
    let request_window_weak = window.as_weak();
    let request_media = Rc::clone(&current_media);
    window.on_open_subtitles_dialog_requested(move || {
        let Some(window) = request_window_weak.upgrade() else {
            return;
        };
        let media = request_media.borrow();
        let Some(video_path) = media.media_path.clone() else {
            tracing::warn!("Open subtitles requested with no video open");
            return;
        };
        let original_path = media
            .subtitle_path
            .clone()
            .or_else(|| open_media_dialog::matching_subtitle_path(&video_path));
        open_subtitles_dialog::open_dialog(
            &window,
            &video_path,
            &open_subtitles_dialog::SubtitleLinks {
                original_path,
                translation_path: media.translation_path.clone(),
            },
        );
    });

    let cancel_window_weak = window.as_weak();
    window.on_cancel_open_subtitles_dialog(move || {
        if let Some(window) = cancel_window_weak.upgrade() {
            window.set_is_open_subtitles_dialog_open(false);
        }
    });

    let confirm_window_weak = window.as_weak();
    window.on_confirm_open_subtitles_dialog(move || {
        if let Some(window) = confirm_window_weak.upgrade() {
            window.set_is_open_subtitles_dialog_open(false);
        }
    });

    let generated_window_weak = window.as_weak();
    let generated_media = Rc::clone(&current_media);
    let generated_state = Rc::clone(state);
    window.on_subtitle_generated(move |subtitle_path| {
        let Some(window) = generated_window_weak.upgrade() else {
            return;
        };
        let subtitle_path = PathBuf::from(subtitle_path.as_str());
        if load_subtitles(&window, &generated_state, &subtitle_path, None) {
            generated_media.borrow_mut().subtitle_path = Some(subtitle_path);
            // Generation can take seconds to minutes (TODO.md Vaihe 21.5),
            // during which the still-playing video may well have reached
            // EOF and left mpv's core idle — a seek issued to an idle core
            // fails outright (mpv error Raw(-12), see video_player.rs's
            // apply_pending_start_seek doc comment). Reloading the video
            // now, the same way opening it fresh does, re-arms the
            // sentence-by-sentence start-of-playback seek onto the
            // newly-loaded first cue and leaves mpv in a normal, seekable
            // state again.
            if let Some(video_path) = generated_media.borrow().media_path.clone() {
                reload_video(&window, &video_path, &generated_state.borrow());
            }
        }
    });

    let generate_window_weak = window.as_weak();
    let generate_media = Rc::clone(&current_media);
    let generate_model = Rc::clone(&selected_model);
    window.on_generate_subtitles_requested(move || {
        let Some(window) = generate_window_weak.upgrade() else {
            return;
        };
        let Some(video_path) = generate_media.borrow().media_path.clone() else {
            tracing::warn!("subtitle generation requested with no video open");
            return;
        };
        let Some(model_path) = generate_model.borrow().clone() else {
            // Defensive fallback: the button is disabled
            // (whisper-model-selected) until a model is picked, so this
            // shouldn't normally be reachable.
            tracing::warn!("subtitle generation requested with no whisper model selected");
            window.set_subtitle_generation_status(SubtitleGenerationStatus::Error);
            window.set_subtitle_generation_error_message("Select a whisper model first.".into());
            return;
        };
        tracing::info!(?video_path, ?model_path, "subtitle generation requested");
        window.set_subtitle_generation_status(SubtitleGenerationStatus::Generating);

        // Only Send data may cross into the background thread and back via
        // slint::invoke_from_event_loop below — a Weak<AppWindow> and the
        // owned generation Result, not the Rc<RefCell<...>> state above.
        // Loading the result into the player happens back on the UI thread
        // via on_subtitle_generated (wired just above), which does have
        // that state.
        let callback_window_weak = generate_window_weak.clone();
        subtitle_generation::spawn_generate(
            whisper_cli_generator(model_path),
            video_path,
            move |result| {
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = callback_window_weak.upgrade() else {
                        return;
                    };
                    let Some(subtitle_path) = subtitle_generation::apply_result(&window, result)
                    else {
                        return;
                    };
                    let Some(subtitle_path) = subtitle_path.to_str() else {
                        tracing::error!(
                            ?subtitle_path,
                            "generated subtitle path is not valid UTF-8"
                        );
                        return;
                    };
                    window.invoke_subtitle_generated(subtitle_path.into());
                });
            },
        );
    });

    let link_entries: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));

    let link_window_weak = window.as_weak();
    let link_media = Rc::clone(&current_media);
    let link_request_entries = Rc::clone(&link_entries);
    window.on_link_translation_requested(move || {
        let Some(window) = link_window_weak.upgrade() else {
            return;
        };
        let Some(video_path) = link_media.borrow().media_path.clone() else {
            return;
        };
        let Some(folder) = video_path.parent().map(Path::to_path_buf) else {
            return;
        };
        let entries = open_subtitles_dialog::list_srt_files(&folder);
        open_subtitles_dialog::open_translation_picker(&window, &folder, &entries);
        *link_request_entries.borrow_mut() = entries;
    });

    let link_select_window_weak = window.as_weak();
    let link_select_entries = Rc::clone(&link_entries);
    window.on_select_link_translation_row(move |index| {
        let Some(window) = link_select_window_weak.upgrade() else {
            return;
        };
        window.set_link_translation_selected_index(index);
        open_subtitles_dialog::mark_translation_selected(
            &window,
            &link_select_entries.borrow(),
            index,
        );
    });

    let link_cancel_window_weak = window.as_weak();
    window.on_cancel_link_translation_dialog(move || {
        if let Some(window) = link_cancel_window_weak.upgrade() {
            window.set_is_link_translation_dialog_open(false);
        }
    });

    let link_confirm_window_weak = window.as_weak();
    let link_confirm_entries = Rc::clone(&link_entries);
    let link_confirm_state = Rc::clone(state);
    let link_confirm_media = Rc::clone(&current_media);
    window.on_confirm_link_translation(move || {
        let Some(window) = link_confirm_window_weak.upgrade() else {
            return;
        };
        let translation_path = usize::try_from(window.get_link_translation_selected_index())
            .ok()
            .and_then(|index| link_confirm_entries.borrow().get(index).cloned());
        let Some(translation_path) = translation_path else {
            return;
        };
        window.set_is_link_translation_dialog_open(false);

        let original_path = link_confirm_media.borrow().subtitle_path.clone();
        let Some(original_path) = original_path else {
            tracing::warn!("cannot link a translation without an original subtitle loaded");
            return;
        };
        if load_subtitles(
            &window,
            &link_confirm_state,
            &original_path,
            Some(&translation_path),
        ) {
            link_confirm_media.borrow_mut().translation_path = Some(translation_path.clone());
            open_subtitles_dialog::mark_translation_linked(&window, &translation_path);
        }
    });
}

/// Wires the Open Subtitles dialog's model row (`TODO.md` Vaihe 21.6):
/// `select-whisper-model-requested` opens a `FileListDialog` scoped to
/// `.bin`/`.gguf` files, starting from `model_picker::default_start_folder`
/// (best-effort autodiscovery, falling back to the config's last-browsed
/// folder or the current working directory) — reusing the same in-app
/// folder-browsing chrome as the Open Video dialog and the translation-link
/// picker (see `open_media_dialog`'s and `open_subtitles_dialog`'s module
/// docs for why there's no OS-native file picker here either).
///
/// The model is deliberately *not* configured through an environment
/// variable the way `TRANGO_WHISPER_CLI_PATH` is (see
/// `whisper_cli_generator`'s doc comment): a learner is expected to switch
/// models/languages fairly often (e.g. one model per target language), so
/// picking one here instead persists it to `config::TrangoConfig`
/// (`crates/app/src/config.rs`) via `config::save`, remembered across
/// restarts without needing to re-set an environment variable each time.
///
/// `selected_model` is shared with `wire_open_subtitles_dialog`'s
/// `generate-subtitles-requested` handler, which reads it when building
/// the generator.
fn wire_model_picker(window: &AppWindow, selected_model: Rc<RefCell<Option<PathBuf>>>) {
    let entries: Rc<RefCell<Vec<model_picker::FolderEntry>>> = Rc::new(RefCell::new(Vec::new()));

    let request_window_weak = window.as_weak();
    let request_entries = Rc::clone(&entries);
    window.on_select_whisper_model_requested(move || {
        let Some(window) = request_window_weak.upgrade() else {
            return;
        };
        let folder = model_picker::default_start_folder(&config::load());
        let files = model_picker::list_folder_entries(&folder);
        model_picker::open_dialog(&window, &folder, &files);
        *request_entries.borrow_mut() = files;
    });

    let select_window_weak = window.as_weak();
    let select_entries = Rc::clone(&entries);
    window.on_select_model_picker_row(move |index| {
        let Some(window) = select_window_weak.upgrade() else {
            return;
        };
        let target_folder = usize::try_from(index).ok().and_then(|index| {
            match select_entries.borrow().get(index)? {
                model_picker::FolderEntry::Up(path)
                | model_picker::FolderEntry::Folder { path, .. } => Some(path.clone()),
                model_picker::FolderEntry::Model(_) => None,
            }
        });
        if let Some(target_folder) = target_folder {
            let files = model_picker::list_folder_entries(&target_folder);
            model_picker::open_dialog(&window, &target_folder, &files);
            *select_entries.borrow_mut() = files;
            return;
        }
        window.set_model_picker_selected_index(index);
        model_picker::mark_selected(&window, &select_entries.borrow(), index);
    });

    let cancel_window_weak = window.as_weak();
    window.on_cancel_model_picker_dialog(move || {
        if let Some(window) = cancel_window_weak.upgrade() {
            window.set_is_model_picker_dialog_open(false);
        }
    });

    let confirm_window_weak = window.as_weak();
    let confirm_entries = Rc::clone(&entries);
    window.on_confirm_model_picker_dialog(move || {
        let Some(window) = confirm_window_weak.upgrade() else {
            return;
        };
        let model = usize::try_from(window.get_model_picker_selected_index())
            .ok()
            .and_then(|index| confirm_entries.borrow().get(index).cloned())
            .and_then(|entry| match entry {
                model_picker::FolderEntry::Model(model) => Some(model),
                _ => None,
            });
        let Some(model) = model else {
            return;
        };
        tracing::info!(model_path = ?model.path, model_name = %model.name, "whisper model selected");
        window.set_is_model_picker_dialog_open(false);
        window.set_whisper_model_selected(true);
        window.set_whisper_model_name(model.name.clone().into());
        *selected_model.borrow_mut() = Some(model.path.clone());

        let mut config = config::load();
        config.whisper_model_path = Some(model.path.clone());
        config.whisper_model_folder = model.path.parent().map(Path::to_path_buf);
        config::save(&config);
    });
}

/// Wires the Settings dialog's Hebrew niqud model row: `select-niqud-model-
/// requested` opens a `FileListDialog` scoped to `.onnx` files, mirroring
/// `wire_model_picker` (see its doc comment for why an in-app folder
/// browser rather than an OS-native picker). Always saves an absolute
/// path (a `read_dir` entry's path always is one) to
/// `config::TrangoConfig::niqud_model_path` — replaces an earlier
/// plain-text field that silently accepted a relative path, which only
/// resolved correctly by accident depending on trango's working directory
/// at launch.
///
/// Unlike the whisper/Ollama pickers, only reachable from the Settings
/// dialog and not shared with any other call site — `niqud_client_from_config`
/// re-reads `config::load()` once at startup, so a pick here takes effect
/// on the next restart, not live; the confirm handler sets
/// `niqud-model-needs-restart` so the Settings dialog says so rather than
/// silently doing nothing until the user figures it out.
fn wire_niqud_model_picker(window: &AppWindow) {
    let entries: Rc<RefCell<Vec<niqud_model_picker::FolderEntry>>> =
        Rc::new(RefCell::new(Vec::new()));

    let request_window_weak = window.as_weak();
    let request_entries = Rc::clone(&entries);
    window.on_select_niqud_model_requested(move || {
        let Some(window) = request_window_weak.upgrade() else {
            return;
        };
        let folder = niqud_model_picker::default_start_folder(&config::load());
        let files = niqud_model_picker::list_folder_entries(&folder);
        niqud_model_picker::open_dialog(&window, &folder, &files);
        *request_entries.borrow_mut() = files;
    });

    let select_window_weak = window.as_weak();
    let select_entries = Rc::clone(&entries);
    window.on_select_niqud_model_picker_row(move |index| {
        let Some(window) = select_window_weak.upgrade() else {
            return;
        };
        let target_folder = usize::try_from(index).ok().and_then(|index| {
            match select_entries.borrow().get(index)? {
                niqud_model_picker::FolderEntry::Up(path)
                | niqud_model_picker::FolderEntry::Folder { path, .. } => Some(path.clone()),
                niqud_model_picker::FolderEntry::Model(_) => None,
            }
        });
        if let Some(target_folder) = target_folder {
            let files = niqud_model_picker::list_folder_entries(&target_folder);
            niqud_model_picker::open_dialog(&window, &target_folder, &files);
            *select_entries.borrow_mut() = files;
            return;
        }
        window.set_niqud_model_picker_selected_index(index);
        niqud_model_picker::mark_selected(&window, &select_entries.borrow(), index);
    });

    let cancel_window_weak = window.as_weak();
    window.on_cancel_niqud_model_picker_dialog(move || {
        if let Some(window) = cancel_window_weak.upgrade() {
            window.set_is_niqud_model_picker_dialog_open(false);
        }
    });

    let confirm_window_weak = window.as_weak();
    let confirm_entries = Rc::clone(&entries);
    window.on_confirm_niqud_model_picker_dialog(move || {
        let Some(window) = confirm_window_weak.upgrade() else {
            return;
        };
        let model = usize::try_from(window.get_niqud_model_picker_selected_index())
            .ok()
            .and_then(|index| confirm_entries.borrow().get(index).cloned())
            .and_then(|entry| match entry {
                niqud_model_picker::FolderEntry::Model(model) => Some(model),
                _ => None,
            });
        let Some(model) = model else {
            return;
        };
        tracing::info!(model_path = ?model.path, model_name = %model.name, "niqud model selected");
        window.set_is_niqud_model_picker_dialog_open(false);
        window.set_niqud_model_selected(true);
        window.set_niqud_model_name(model.name.clone().into());
        window.set_niqud_model_needs_restart(true);

        let mut config = config::load();
        config.niqud_model_path = Some(model.path.clone());
        config::save(&config);
    });
}

/// Wires the Open Subtitles dialog's Ollama model row (`TODO.md` Vaihe 24,
/// part 3/6): `select-ollama-model-requested` opens a `FileListDialog`
/// listing models a local Ollama instance reports installed
/// (`word_analysis::OllamaClient::list_models`), fetched on a background
/// thread via `ollama_model_picker::spawn_list_models` since it's a
/// network call, unlike the whisper model picker's synchronous filesystem
/// listing — see that module's doc comment. Picking a model persists it to
/// `config::TrangoConfig::ollama_model`, the same way `wire_model_picker`
/// persists the whisper model.
///
/// `selected_ollama_model` is shared with the Ctrl+A popup and "Analyze
/// all sentences" wiring (`TODO.md` Vaihe 24, parts 5-6), which read it
/// when building an `OllamaClient` call.
fn wire_ollama_model_picker(
    window: &AppWindow,
    selected_ollama_model: Rc<RefCell<Option<String>>>,
) {
    let request_window_weak = window.as_weak();
    let request_current_model = Rc::clone(&selected_ollama_model);
    window.on_select_ollama_model_requested(move || {
        let Some(window) = request_window_weak.upgrade() else {
            return;
        };
        ollama_model_picker::open_dialog_loading(&window);

        // Only Send data may cross into the background thread and back via
        // slint::invoke_from_event_loop below (see subtitle_generation.rs's
        // identical note) — an owned Option<String>, not the
        // Rc<RefCell<...>> state above. The freshly listed models
        // themselves end up in ollama-model-picker-rows (a Slint model,
        // updated on the UI thread inside apply_models_result), so the
        // select/confirm handlers below read model names back out of that
        // instead of needing their own Rc<RefCell<Vec<String>>>.
        let current_model = request_current_model.borrow().clone();
        let callback_window_weak = request_window_weak.clone();
        ollama_model_picker::spawn_list_models(
            ::word_analysis::HttpOllamaClient::default(),
            move |result| {
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = callback_window_weak.upgrade() else {
                        return;
                    };
                    ollama_model_picker::apply_models_result(
                        &window,
                        result,
                        current_model.as_deref(),
                    );
                });
            },
        );
    });

    let select_window_weak = window.as_weak();
    window.on_select_ollama_model_picker_row(move |index| {
        let Some(window) = select_window_weak.upgrade() else {
            return;
        };
        let models: Vec<String> = window
            .get_ollama_model_picker_rows()
            .iter()
            .map(|row| row.name.to_string())
            .collect();
        ollama_model_picker::mark_selected(&window, &models, index);
    });

    let cancel_window_weak = window.as_weak();
    window.on_cancel_ollama_model_picker_dialog(move || {
        if let Some(window) = cancel_window_weak.upgrade() {
            window.set_is_ollama_model_picker_open(false);
        }
    });

    let confirm_window_weak = window.as_weak();
    window.on_confirm_ollama_model_picker_dialog(move || {
        let Some(window) = confirm_window_weak.upgrade() else {
            return;
        };
        let model = usize::try_from(window.get_ollama_model_picker_selected_index())
            .ok()
            .and_then(|index| window.get_ollama_model_picker_rows().row_data(index))
            .map(|row| row.name.to_string());
        let Some(model) = model else {
            return;
        };
        tracing::info!(%model, "Ollama model selected");
        window.set_is_ollama_model_picker_open(false);
        window.set_ollama_model_selected(true);
        window.set_ollama_model_name(model.clone().into());
        *selected_ollama_model.borrow_mut() = Some(model.clone());

        let mut config = config::load();
        config.ollama_model = Some(model);
        config::save(&config);
    });
}

/// Wires the Open Subtitles dialog's target-language field (`TODO.md`
/// Vaihe 24.1): `set-ollama-target-language`, invoked on every keystroke
/// (the `LineEdit`'s `edited` callback), updates the shared
/// `target_language` state `wire_word_analysis_batch`/
/// `wire_word_analysis_popup` read from and persists it to
/// `config::TrangoConfig::ollama_target_language`, the same way picking
/// an Ollama model persists immediately rather than waiting for a
/// separate "Save" action.
fn wire_ollama_target_language(window: &AppWindow, target_language: Rc<RefCell<String>>) {
    window.on_set_ollama_target_language(move |language| {
        let language = language.to_string();
        *target_language.borrow_mut() = language.clone();

        let mut config = config::load();
        config.ollama_target_language = Some(language);
        config::save(&config);
    });
}

/// Wires the top bar's Settings gear (`settings-dialog-requested`) and the
/// resulting dialog's callbacks. Opening loads the current config.toml
/// into the dialog's display properties (`settings_dialog::open_dialog`);
/// editing video-folder/audio-monitor-source/audio-recording-folder
/// persists immediately, the same way `wire_ollama_target_language` above
/// does, and (for the recording folder) refreshes the Audio panel's
/// "Saving to:" label (`system_audio_capture::refresh_recording_folder_label`)
/// so a folder change is visible there without restarting the app.
/// Whisper/Ollama model selection and the target-language field aren't
/// wired here at all — the dialog's `select-whisper-model`/
/// `select-ollama-model`/`set-ollama-target-language` forward straight to
/// `select-whisper-model-requested`/`select-ollama-model-requested`/
/// `set-ollama-target-language` (`app-window.slint`'s SettingsDialog
/// instantiation), already handled by `wire_model_picker`/
/// `wire_ollama_model_picker`/`wire_ollama_target_language`.
fn wire_settings_dialog(window: &AppWindow) {
    let open_window_weak = window.as_weak();
    window.on_settings_dialog_requested(move || {
        let Some(window) = open_window_weak.upgrade() else {
            return;
        };
        settings_dialog::open_dialog(&window, &config::load());
    });

    let close_window_weak = window.as_weak();
    window.on_close_settings_dialog(move || {
        if let Some(window) = close_window_weak.upgrade() {
            window.set_is_settings_dialog_open(false);
        }
    });

    let video_folder_window_weak = window.as_weak();
    window.on_set_settings_video_folder(move |folder| {
        let Some(window) = video_folder_window_weak.upgrade() else {
            return;
        };
        let folder = folder.to_string();
        let trimmed = folder.trim();
        let mut config = config::load();
        config.video_folder = (!trimmed.is_empty()).then(|| PathBuf::from(trimmed));
        config::save(&config);
        window.set_settings_video_folder(folder.into());
    });

    let monitor_window_weak = window.as_weak();
    window.on_set_settings_audio_monitor_source(move |source| {
        let Some(window) = monitor_window_weak.upgrade() else {
            return;
        };
        let source = source.to_string();
        let mut config = config::load();
        config.audio_monitor_source =
            (!source.trim().is_empty()).then(|| source.trim().to_string());
        config::save(&config);
        window.set_settings_audio_monitor_source(source.into());
    });

    let folder_window_weak = window.as_weak();
    window.on_set_settings_audio_recording_folder(move |folder| {
        let Some(window) = folder_window_weak.upgrade() else {
            return;
        };
        let folder = folder.to_string();
        let trimmed = folder.trim();
        let mut config = config::load();
        config.audio_recording_folder = (!trimmed.is_empty()).then(|| PathBuf::from(trimmed));
        config::save(&config);

        window.set_settings_audio_recording_folder(folder.into());
        window.set_settings_audio_recording_folder_exists(
            system_audio_capture::default_recording_folder(&config).is_dir(),
        );
        system_audio_capture::refresh_recording_folder_label(&window, &config);
    });
}

/// Wires the Open Subtitles dialog's "Analyze all sentences" button
/// (`TODO.md` Vaihe 24, part 4/6): `analyze-all-requested` runs
/// `word_analysis::spawn_batch_analyze` over every cue in the currently
/// loaded subtitle, saving newly analyzed cues to that subtitle's
/// `word_analysis::cache_path_for` file as it goes. A no-op (with a
/// user-visible error) if no Ollama model is selected or no subtitle is
/// loaded — the button is disabled for the former case already
/// (`ollama-model-selected`), but the callback still guards it
/// defensively since Slint's `enabled` is advisory, not enforced.
fn wire_word_analysis_batch(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    current_media: &Rc<RefCell<CurrentMedia>>,
    selected_ollama_model: Rc<RefCell<Option<String>>>,
    target_language: Rc<RefCell<String>>,
    niqud_client: Option<niqud::OnnxNiqudClient>,
) {
    let window_weak = window.as_weak();
    let state = Rc::clone(state);
    let current_media = Rc::clone(current_media);
    window.on_analyze_all_requested(move || {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let Some(model) = selected_ollama_model.borrow().clone() else {
            tracing::warn!("analyze-all requested with no Ollama model selected");
            window.set_word_analysis_batch_status(WordAnalysisBatchStatus::Error);
            window.set_word_analysis_batch_error_message("Select an Ollama model first.".into());
            return;
        };
        let Some(subtitle_path) = current_media.borrow().subtitle_path.clone() else {
            tracing::warn!("analyze-all requested with no subtitle loaded");
            window.set_word_analysis_batch_status(WordAnalysisBatchStatus::Error);
            window.set_word_analysis_batch_error_message("Link a subtitle first.".into());
            return;
        };
        let cues = state.borrow().cues.clone();
        let cache_path = ::word_analysis::cache_path_for(&subtitle_path);
        window.set_word_analysis_batch_status(WordAnalysisBatchStatus::Running);
        window.set_word_analysis_batch_progress_current(0);
        window.set_word_analysis_batch_progress_total(cues.len() as i32);
        window.set_word_analysis_batch_error_message("".into());

        // Only Send data may cross into the background thread and back via
        // slint::invoke_from_event_loop below — see wire_ollama_model_picker's
        // identical note.
        let progress_window_weak = window_weak.clone();
        let done_window_weak = window_weak.clone();
        word_analysis::spawn_batch_analyze(
            word_analysis::AnalysisClients {
                ollama: ::word_analysis::HttpOllamaClient::default(),
                niqud: niqud_client.clone(),
            },
            model,
            target_language.borrow().clone(),
            cues,
            cache_path,
            move |done, total| {
                let progress_window_weak = progress_window_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = progress_window_weak.upgrade() else {
                        return;
                    };
                    word_analysis::apply_batch_progress(&window, done, total);
                });
            },
            move |result| {
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = done_window_weak.upgrade() else {
                        return;
                    };
                    word_analysis::apply_batch_result(&window, result);
                });
            },
        );
    });
}

/// Wires the Ctrl+A word-analysis popup (`TODO.md` Vaihe 24, part 5/6):
/// `show-word-analysis` resolves the sentence currently shown in the
/// current-sentence card (`PlayerState::current_cue_index`), checks the
/// subtitle's cache file — freshly read from disk each time, cheap for a
/// small JSON file and always reflects whatever "Analyze all sentences"
/// or an earlier Ctrl+A press already wrote, without needing a separate
/// in-memory cache kept in sync across both paths — and either shows a
/// cache hit immediately or kicks off `word_analysis::spawn_analyze_sentence`,
/// writing its result into that same cache file once it reports back so
/// the next lookup (Ctrl+A again, or a later "Analyze all sentences" run)
/// is a cache hit too.
fn wire_word_analysis_popup(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    current_media: &Rc<RefCell<CurrentMedia>>,
    selected_ollama_model: Rc<RefCell<Option<String>>>,
    target_language: Rc<RefCell<String>>,
    niqud_client: Option<niqud::OnnxNiqudClient>,
) {
    let window_weak = window.as_weak();
    let request_state = Rc::clone(state);
    let request_media = Rc::clone(current_media);
    window.on_show_word_analysis(move || {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let Some(model) = selected_ollama_model.borrow().clone() else {
            tracing::warn!("word analysis requested with no Ollama model selected");
            window.set_word_analysis_status(WordAnalysisStatus::Error);
            window.set_word_analysis_error_message("Select an Ollama model first.".into());
            window.set_is_word_analysis_popup_open(true);
            return;
        };
        let Some(subtitle_path) = request_media.borrow().subtitle_path.clone() else {
            tracing::warn!("word analysis requested with no subtitle loaded");
            window.set_word_analysis_status(WordAnalysisStatus::Error);
            window.set_word_analysis_error_message("Link a subtitle first.".into());
            window.set_is_word_analysis_popup_open(true);
            return;
        };
        // panel_content_ready guards against analyzing a sentence that
        // isn't actually being shown — e.g. Ctrl+A pressed right after
        // switching to an Audio panel with nothing loaded there yet, where
        // the current-sentence card was just blanked by
        // wire_player_state's on_select_media_source but PlayerState's
        // cues/cursor are still whatever the previous source left behind.
        let cue = if panel_content_ready(&window) {
            let state = request_state.borrow();
            state
                .current_cue_index
                .and_then(|index| state.cues.get(index).cloned())
        } else {
            None
        };
        let Some(cue) = cue else {
            tracing::warn!("word analysis requested with no sentence in focus");
            window.set_word_analysis_status(WordAnalysisStatus::Error);
            window.set_word_analysis_error_message("No sentence is currently in focus.".into());
            window.set_is_word_analysis_popup_open(true);
            return;
        };

        let cache_path = ::word_analysis::cache_path_for(&subtitle_path);
        let cache = ::word_analysis::load_cache(&cache_path);
        // An entry with no words is what spawn_batch_analyze leaves behind
        // for a cue that kept failing after all its retries — not treating
        // it as a hit means pressing Ctrl+A on that sentence tries Ollama
        // again instead of reopening the same blank popup forever.
        if let Some(analysis) = cache
            .entries
            .get(&cue.index)
            .filter(|a| !a.words.is_empty())
        {
            tracing::debug!(cue_index = cue.index, "word analysis cache hit");
            word_analysis::open_popup_with_result(&window, analysis);
            return;
        }

        word_analysis::open_popup_loading(&window);

        // Only Send data may cross into the background thread and back via
        // slint::invoke_from_event_loop below — see wire_ollama_model_picker's
        // identical note.
        let callback_window_weak = window_weak.clone();
        let cue_index = cue.index;
        word_analysis::spawn_analyze_sentence(
            ::word_analysis::HttpOllamaClient::default(),
            niqud_client.clone(),
            model,
            cue.text.clone(),
            target_language.borrow().clone(),
            move |result| {
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = callback_window_weak.upgrade() else {
                        return;
                    };
                    word_analysis::apply_single_result(&window, &result);
                    if let Ok(analysis) = &result {
                        let mut cache = ::word_analysis::load_cache(&cache_path);
                        cache.entries.insert(cue_index, analysis.clone());
                        ::word_analysis::save_cache(&cache_path, &cache);
                    }
                });
            },
        );
    });

    let close_window_weak = window.as_weak();
    window.on_close_word_analysis_popup(move || {
        if let Some(window) = close_window_weak.upgrade() {
            window.set_is_word_analysis_popup_open(false);
        }
    });
}

/// Wires the Ctrl+W word-timing popup (`TODO.md` Vaihe 32): `show-word-timing`
/// resolves the current cue/media/whisper model the same way
/// `wire_word_analysis_popup` resolves the current cue/subtitle/Ollama
/// model, then runs `word_timing_ui::spawn_segment_words` on a background
/// thread and applies its result once finished. `close-word-timing-popup`
/// just hides the popup. `play-word-timing` plays back the clicked row's
/// exact audio span via `play_span` — the same bounded-span playback
/// `wire_cue_navigation`'s `repeat-cue` uses for a whole cue
/// (`video_player::VideoPlayer::toggle_play_span`), reused here for one
/// word. Taking a plain closure rather than a `Rc<video_player::VideoPlayer>`
/// directly keeps this function testable without a real mpv render
/// context, same reasoning as `wire_open_subtitles_dialog`'s
/// `reload_video` parameter (see its doc comment).
fn wire_word_timing_popup(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    current_media: &Rc<RefCell<CurrentMedia>>,
    selected_model: Rc<RefCell<Option<PathBuf>>>,
    play_span: impl Fn(PlaySpanCommand) + 'static,
) {
    let window_weak = window.as_weak();
    let request_state = Rc::clone(state);
    let request_media = Rc::clone(current_media);
    window.on_show_word_timing(move || {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let Some(model_path) = selected_model.borrow().clone() else {
            tracing::warn!("word timing requested with no whisper model selected");
            window.set_word_timing_status(WordTimingStatus::Error);
            window.set_word_timing_error_message("Select a whisper model first.".into());
            window.set_is_word_timing_popup_open(true);
            return;
        };
        let Some(media_path) = request_media.borrow().media_path.clone() else {
            tracing::warn!("word timing requested with no video or audio file open");
            window.set_word_timing_status(WordTimingStatus::Error);
            window.set_word_timing_error_message("Open a video or audio file first.".into());
            window.set_is_word_timing_popup_open(true);
            return;
        };
        // panel_content_ready guards against segmenting a sentence that
        // isn't actually being shown, same as wire_word_analysis_popup's
        // identical check — an empty PlayerState.cues (no subtitle
        // loaded) also naturally falls into the "no cue" branch below.
        let cue = if panel_content_ready(&window) {
            let state = request_state.borrow();
            state
                .current_cue_index
                .and_then(|index| state.cues.get(index).cloned())
        } else {
            None
        };
        let Some(cue) = cue else {
            tracing::warn!("word timing requested with no sentence in focus");
            window.set_word_timing_status(WordTimingStatus::Error);
            window.set_word_timing_error_message("No sentence is currently in focus.".into());
            window.set_is_word_timing_popup_open(true);
            return;
        };

        word_timing_ui::open_popup_loading(&window);

        // Only Send data may cross into the background thread and back via
        // slint::invoke_from_event_loop below — see wire_word_analysis_popup's
        // identical note.
        let callback_window_weak = window_weak.clone();
        word_timing_ui::spawn_segment_words(
            whisper_cli_word_segmenter(model_path),
            media_path,
            cue.start,
            cue.end,
            move |result| {
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = callback_window_weak.upgrade() else {
                        return;
                    };
                    word_timing_ui::apply_result(&window, result);
                });
            },
        );
    });

    let close_window_weak = window.as_weak();
    window.on_close_word_timing_popup(move || {
        if let Some(window) = close_window_weak.upgrade() {
            window.set_is_word_timing_popup_open(false);
        }
    });

    window.on_play_word_timing(move |start_seconds, end_seconds| {
        play_span(PlaySpanCommand {
            start: std::time::Duration::from_secs_f32(start_seconds),
            end: std::time::Duration::from_secs_f32(end_seconds),
        });
    });
}

fn main() -> anyhow::Result<()> {
    let (debug, args) = extract_debug_flag(std::env::args().collect());
    init_logging(debug);
    tracing::info!("trango starting");
    print_version();

    let window = AppWindow::new()?;
    window.set_version(env!("CARGO_PKG_VERSION").into());
    let player_state = wire_player_state(&window);
    sentence_card::update_sentence_card(&window, &player_state.borrow());
    sentence_list::update_sentence_list(&window, &player_state.borrow());

    let current_media = Rc::new(RefCell::new(CurrentMedia::default()));

    if let Some(subtitle_path) = subtitle_path_from_args(&args) {
        let translation_path = translation_path_from_args(&args);
        let loaded = load_subtitles(
            &window,
            &player_state,
            &subtitle_path,
            translation_path.as_deref(),
        );
        if loaded {
            let mut media = current_media.borrow_mut();
            media.subtitle_path = Some(subtitle_path);
            media.translation_path = translation_path;
        }
    }

    let video_path = video_path_from_args(&args);
    if video_path.is_none() {
        tracing::info!(
            "no video path given; use the \"Open…\" button or run as `trango <path/to/video>`"
        );
    }
    current_media.borrow_mut().media_path = video_path.clone();
    let video_player = Rc::new(video_player::VideoPlayer::attach(
        &window,
        video_path.as_deref(),
        Rc::clone(&player_state),
    )?);
    wire_cue_navigation(&window, &player_state, Rc::clone(&video_player));
    wire_scrub_bar(&window, Rc::clone(&video_player));
    wire_speed_slider(&window, Rc::clone(&video_player));
    wire_pause_playback(&window, Rc::clone(&video_player));

    let startup_config = config::load();
    let niqud_client = niqud_client_from_config(&startup_config);

    wire_open_media_dialog(
        &window,
        &player_state,
        Rc::clone(&video_player),
        default_video_folder(&args, &startup_config),
        Rc::clone(&current_media),
    );

    let selected_model = Rc::new(RefCell::new(
        startup_config
            .whisper_model_path
            .filter(|path| path.is_file()),
    ));
    if let Some(model_path) = selected_model.borrow().clone() {
        window.set_whisper_model_selected(true);
        window.set_whisper_model_name(model_picker::display_name(&model_path).into());
    }
    wire_model_picker(&window, Rc::clone(&selected_model));

    let selected_ollama_model = Rc::new(RefCell::new(startup_config.ollama_model.clone()));
    if let Some(model) = selected_ollama_model.borrow().clone() {
        window.set_ollama_model_selected(true);
        window.set_ollama_model_name(model.into());
    }
    wire_ollama_model_picker(&window, Rc::clone(&selected_ollama_model));

    if let Some(niqud_model_path) = startup_config
        .niqud_model_path
        .as_deref()
        .filter(|path| path.is_file())
    {
        window.set_niqud_model_selected(true);
        window.set_niqud_model_name(
            niqud_model_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| niqud_model_path.display().to_string())
                .into(),
        );
    }
    wire_niqud_model_picker(&window);

    let target_language = Rc::new(RefCell::new(
        startup_config
            .ollama_target_language
            .clone()
            .unwrap_or_else(|| word_analysis::DEFAULT_TARGET_LANGUAGE.to_string()),
    ));
    window.set_ollama_target_language(target_language.borrow().clone().into());
    wire_ollama_target_language(&window, Rc::clone(&target_language));
    wire_settings_dialog(&window);

    wire_word_analysis_batch(
        &window,
        &player_state,
        &current_media,
        Rc::clone(&selected_ollama_model),
        Rc::clone(&target_language),
        niqud_client.clone(),
    );
    wire_word_analysis_popup(
        &window,
        &player_state,
        &current_media,
        Rc::clone(&selected_ollama_model),
        Rc::clone(&target_language),
        niqud_client,
    );
    let word_timing_video_player = Rc::clone(&video_player);
    wire_word_timing_popup(
        &window,
        &player_state,
        &current_media,
        Rc::clone(&selected_model),
        move |command| word_timing_video_player.toggle_play_span(command),
    );

    let mut startup_audio_capture = audio_capture::AudioCapture::default();
    startup_audio_capture.ffmpeg_path = ffmpeg_path_from_env();
    let recording_video_player = Rc::clone(&video_player);
    let recording_state = Rc::clone(&player_state);
    let recording_media = Rc::clone(&current_media);
    system_audio_capture::wire_audio_capture(
        &window,
        startup_audio_capture,
        move |window, recording_path| {
            open_selected_media(
                window,
                &recording_state,
                &recording_video_player,
                open_media_dialog::MediaKind::Audio,
                recording_path,
                &recording_media,
            );
        },
    );

    let reload_video_player = Rc::clone(&video_player);
    wire_open_subtitles_dialog(
        &window,
        &player_state,
        move |window, video_path, state| reload_video_player.load_video(window, video_path, state),
        current_media,
        selected_model,
    );

    window.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use slint::Model;
    use subtitle::SubtitleGenerator;

    use super::*;

    /// Restores `.0` to the real config.toml (`config::save`) when
    /// dropped — including when a test panics partway through, unlike a
    /// bare `config::save(&original_config)` at the end of a test block,
    /// which never runs if an assertion in between it and the start of
    /// the block panics. That gap is exactly how a stale
    /// `audio_recording_folder` pointing at an already-deleted temp test
    /// directory once ended up persisted to a real developer's
    /// config.toml — only noticed because the Settings screen's new
    /// audio-recording-folder-exists check started surfacing it visibly.
    /// Tests that temporarily overwrite config.toml should bind one of
    /// these right after capturing `config::load()`, so the restore
    /// happens no matter how the rest of the test exits.
    struct ConfigRestoreGuard(config::TrangoConfig);

    impl Drop for ConfigRestoreGuard {
        fn drop(&mut self) {
            config::save(&self.0);
        }
    }

    #[test]
    fn test_version_is_set() {
        // Given: the crate's compiled version metadata
        // When:  reading CARGO_PKG_VERSION
        // Then:  it is non-empty, proving the version is wired up for display
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    #[test]
    fn test_extract_debug_flag_present() {
        // Given: argv with --debug mixed in among positional args
        // When:  extracting the flag
        // Then:  it's reported present, and removed from the remaining args
        //        so positional indices (video/subtitle/translation) still
        //        line up
        let args = vec![
            "trango".to_string(),
            "video.mp4".to_string(),
            "--debug".to_string(),
            "subs.srt".to_string(),
        ];

        let (debug, remaining) = extract_debug_flag(args);

        assert!(debug);
        assert_eq!(
            remaining,
            vec![
                "trango".to_string(),
                "video.mp4".to_string(),
                "subs.srt".to_string()
            ]
        );
    }

    #[test]
    fn test_niqud_client_from_config_returns_none_when_not_configured() {
        // Given: a config with no niqud_model_path set
        // When:  building the niqud client
        // Then:  None comes back without spawning anything
        let config = config::TrangoConfig::default();

        assert!(niqud_client_from_config(&config).is_none());
    }

    #[test]
    fn test_niqud_client_from_config_returns_none_quickly_for_a_bad_path() {
        // Given: a config pointing at a model file that doesn't exist
        // When:  building the niqud client
        // Then:  None comes back well within NIQUD_LOAD_TIMEOUT — proving
        //        a normal load failure (missing file) is reported via the
        //        fast Ok(Err(_)) path, not by waiting out the timeout
        //        meant only for a genuine hang
        let config = config::TrangoConfig {
            niqud_model_path: Some(PathBuf::from("/no/such/trango-test-niqud-model.onnx")),
            ..Default::default()
        };

        let started = std::time::Instant::now();
        let client = niqud_client_from_config(&config);
        let elapsed = started.elapsed();

        assert!(client.is_none());
        assert!(
            elapsed < NIQUD_LOAD_TIMEOUT / 2,
            "expected a fast failure, took {elapsed:?}"
        );
    }

    #[test]
    fn test_extract_debug_flag_absent() {
        // Given: argv with no --debug flag
        // When:  extracting the flag
        // Then:  it's reported absent, and args come back unchanged
        let args = vec!["trango".to_string(), "video.mp4".to_string()];

        let (debug, remaining) = extract_debug_flag(args.clone());

        assert!(!debug);
        assert_eq!(remaining, args);
    }

    #[test]
    fn test_video_path_from_args_with_path() {
        // Given: argv as std::env::args() would yield it, program name + one path
        let args = vec!["trango".to_string(), "video.mp4".to_string()];

        // When:  extracting the video path
        // Then:  it's the first argument after the program name
        assert_eq!(
            video_path_from_args(&args),
            Some(PathBuf::from("video.mp4"))
        );
    }

    #[test]
    fn test_video_path_from_args_without_path() {
        // Given: argv with only the program name, i.e. no video was requested
        let args = vec!["trango".to_string()];

        // When:  extracting the video path
        // Then:  there is none
        assert_eq!(video_path_from_args(&args), None);
    }

    #[test]
    fn test_subtitle_path_from_args_with_both_paths() {
        // Given: argv with a video path followed by a subtitle path
        let args = vec![
            "trango".to_string(),
            "video.mp4".to_string(),
            "subs.srt".to_string(),
        ];

        // When:  extracting the subtitle path
        // Then:  it's the second argument after the program name
        assert_eq!(
            subtitle_path_from_args(&args),
            Some(PathBuf::from("subs.srt"))
        );
    }

    #[test]
    fn test_subtitle_path_from_args_without_subtitle_path() {
        // Given: argv with only a video path, no subtitle path
        let args = vec!["trango".to_string(), "video.mp4".to_string()];

        // When:  extracting the subtitle path
        // Then:  there is none
        assert_eq!(subtitle_path_from_args(&args), None);
    }

    #[test]
    fn test_translation_path_from_args_with_all_three_paths() {
        // Given: argv with a video path, a subtitle path, and a translation path
        let args = vec![
            "trango".to_string(),
            "video.mp4".to_string(),
            "subs.srt".to_string(),
            "subs.en.srt".to_string(),
        ];

        // When:  extracting the translation path
        // Then:  it's the third argument after the program name
        assert_eq!(
            translation_path_from_args(&args),
            Some(PathBuf::from("subs.en.srt"))
        );
    }

    #[test]
    fn test_translation_path_from_args_without_translation_path() {
        // Given: argv with a video path and a subtitle path, no translation path
        let args = vec![
            "trango".to_string(),
            "video.mp4".to_string(),
            "subs.srt".to_string(),
        ];

        // When:  extracting the translation path
        // Then:  there is none
        assert_eq!(translation_path_from_args(&args), None);
    }

    #[test]
    fn test_default_video_folder_with_cli_video_path() {
        // Given: argv with a video path that has a parent directory, and a
        //        config with a different saved video folder
        let args = vec!["trango".to_string(), "some/folder/video.mp4".to_string()];
        let config = config::TrangoConfig {
            video_folder: Some(PathBuf::from("/saved/folder")),
            ..Default::default()
        };

        // When:  resolving the Open dialog's default Video-mode folder
        // Then:  the CLI video's parent directory wins over the saved folder
        assert_eq!(
            default_video_folder(&args, &config),
            PathBuf::from("some/folder")
        );
    }

    #[test]
    fn test_default_video_folder_without_cli_video_path_uses_saved_folder() {
        // Given: argv with no video path, and a config with a saved video
        //        folder from a previous run
        let args = vec!["trango".to_string()];
        let config = config::TrangoConfig {
            video_folder: Some(PathBuf::from("/saved/folder")),
            ..Default::default()
        };

        // When:  resolving the Open dialog's default Video-mode folder
        // Then:  it's the saved folder
        assert_eq!(
            default_video_folder(&args, &config),
            PathBuf::from("/saved/folder")
        );
    }

    #[test]
    fn test_default_video_folder_without_cli_video_path_or_saved_folder() {
        // Given: argv with no video path, and no saved video folder either
        let args = vec!["trango".to_string()];

        // When:  resolving the Open dialog's default Video-mode folder
        // Then:  it falls back to the current working directory
        assert_eq!(
            default_video_folder(&args, &config::TrangoConfig::default()),
            std::env::current_dir().expect("failed to read current directory")
        );
    }

    #[test]
    fn test_default_video_folder_with_bare_filename_falls_back_to_cwd() {
        // Given: argv with a video path that has no parent directory
        //        component (a bare filename), and no saved video folder
        let args = vec!["trango".to_string(), "video.mp4".to_string()];

        // When:  resolving the Open dialog's default Video-mode folder
        // Then:  it falls back to the current working directory, since
        //        "video.mp4"'s parent is the empty path, not a real folder
        assert_eq!(
            default_video_folder(&args, &config::TrangoConfig::default()),
            std::env::current_dir().expect("failed to read current directory")
        );
    }

    #[test]
    fn test_media_kind_for_source_matches_source() {
        // Given/When/Then: each MediaSource maps to the same-named
        //                   MediaKind, since the two sources share one
        //                   Open dialog distinguished only by which kind
        //                   of file it lists
        assert_eq!(
            media_kind_for_source(MediaSource::Video),
            open_media_dialog::MediaKind::Video
        );
        assert_eq!(
            media_kind_for_source(MediaSource::Audio),
            open_media_dialog::MediaKind::Audio
        );
    }

    #[test]
    fn test_open_media_dialog_title_names_the_kind() {
        // Given/When/Then: the dialog's title names whichever kind of file
        //                   it's currently listing
        assert_eq!(
            open_media_dialog_title(open_media_dialog::MediaKind::Video),
            "Open video file"
        );
        assert_eq!(
            open_media_dialog_title(open_media_dialog::MediaKind::Audio),
            "Open audio file"
        );
    }

    // Slint's winit backend can only be initialized once per process (and
    // stays bound to the thread that created it), so every assertion that
    // needs a real `AppWindow` lives in this single test instead of one
    // `AppWindow::new()` call per test — a second call from cargo test's
    // per-test thread fails with "platform was initialized in another
    // thread" / "EventLoop can't be recreated".
    #[test]
    fn test_app_window_properties() {
        // Given: a freshly constructed AppWindow
        let window = AppWindow::new().expect("failed to create AppWindow");

        // When:  the version property is set to CARGO_PKG_VERSION
        // Then:  reading it back returns the same value
        window.set_version(env!("CARGO_PKG_VERSION").into());
        assert_eq!(window.get_version(), env!("CARGO_PKG_VERSION"));

        // When:  reading playback_mode before wiring
        // Then:  it's still app-window.slint's own hardcoded default (Normal)
        assert_eq!(window.get_playback_mode(), PlaybackModeUi::Normal);
        assert!(!window.get_sentence_mode_active());

        // When:  wiring a fresh PlayerState
        // Then:  it defaults to SentenceBySentence mode (the primary
        //        language-learning use case) and Video source, mirrored
        //        into playback_mode/media_source
        let player_state = wire_player_state(&window);
        assert_eq!(player_state.borrow().mode, PlaybackMode::SentenceBySentence);
        assert_eq!(
            window.get_playback_mode(),
            PlaybackModeUi::SentenceBySentence
        );
        assert!(window.get_sentence_mode_active());
        assert_eq!(player_state.borrow().media_source, MediaSource::Video);
        assert_eq!(window.get_media_source(), MediaSourceUi::Video);

        // When:  invoking select-mode(Normal), as a segmented control click
        //        on the "Normal" segment does
        // Then:  both the Rust-owned PlayerState and the mirrored Slint
        //        property switch to Normal
        window.invoke_select_mode(PlaybackModeUi::Normal);
        assert_eq!(player_state.borrow().mode, PlaybackMode::Normal);
        assert_eq!(window.get_playback_mode(), PlaybackModeUi::Normal);
        assert!(!window.get_sentence_mode_active());

        // When:  invoking select-media-source(Audio), as the Video/Audio
        //        segmented control's "Audio" segment does
        // Then:  both the Rust-owned PlayerState and the mirrored Slint
        //        property switch to Audio, independently of playback_mode
        window.invoke_select_media_source(MediaSourceUi::Audio);
        assert_eq!(player_state.borrow().media_source, MediaSource::Audio);
        assert_eq!(window.get_media_source(), MediaSourceUi::Audio);
        assert_eq!(player_state.borrow().mode, PlaybackMode::Normal);

        // When:  invoking select-media-source(Video) again
        // Then:  both flip back to Video
        window.invoke_select_media_source(MediaSourceUi::Video);
        assert_eq!(player_state.borrow().media_source, MediaSource::Video);
        assert_eq!(window.get_media_source(), MediaSourceUi::Video);

        // When:  a video is loaded (video-loaded + loaded-media-source both
        //        default to Video, matching the Video source already
        //        active)
        // Then:  media-ready is true — the mpv underlay/ScrubBar/SpeedSlider
        //        should show
        window.set_video_loaded(true);
        assert!(window.get_media_ready());

        // When:  switching to the Audio source without loading anything
        //        there (as a bare Video/Audio click does — the loaded video
        //        is still the same mpv instance's file, see media-ready's
        //        doc comment)
        // Then:  media-ready is false, so the Audio panel doesn't show the
        //        stale video's ScrubBar or let its picture bleed through
        window.invoke_select_media_source(MediaSourceUi::Audio);
        assert!(!window.get_media_ready());

        // When:  an audio recording is then loaded, mirroring
        //        loaded-media-source to Audio (as open_selected_media does)
        // Then:  media-ready is true again, now for the Audio source
        window.set_loaded_media_source(MediaSourceUi::Audio);
        assert!(window.get_media_ready());

        // When:  switching back to Video
        // Then:  media-ready is false again — the loaded file is audio, not
        //        video
        window.invoke_select_media_source(MediaSourceUi::Video);
        assert!(!window.get_media_ready());

        // When:  invoking select-mode(SentenceBySentence) again
        // Then:  both flip back to SentenceBySentence
        window.invoke_select_mode(PlaybackModeUi::SentenceBySentence);
        assert_eq!(player_state.borrow().mode, PlaybackMode::SentenceBySentence);
        assert!(window.get_sentence_mode_active());

        // When:  the sentence card is wired to a state with no cues loaded
        // Then:  it shows the placeholder label/text
        sentence_card::update_sentence_card(&window, &player_state.borrow());
        assert_eq!(window.get_sentence_label(), "Sentence – / –");
        assert!(!window.get_has_current_sentence());

        // When:  loading the real sample.srt fixture via load_subtitles, with
        //        no translation path
        // Then:  the card's properties reflect the first parsed cue, with no
        //        translation text
        let subtitle_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/sample/sample.srt");
        load_subtitles(&window, &player_state, &subtitle_path, None);
        assert_eq!(player_state.borrow().cues.len(), 5);
        assert_eq!(window.get_sentence_label(), "Sentence 1 / 5");
        assert_eq!(window.get_sentence_text(), "Welcome to Trango Player.");
        assert!(window.get_has_current_sentence());
        assert_eq!(window.get_translation_text(), "");

        // When:  the same load_subtitles call also feeds the sentence list
        // Then:  it holds one row per cue, the first one marked current
        let rows = window.get_sentence_list_rows();
        assert_eq!(rows.row_count(), 5);
        let first_row = rows.row_data(0).expect("row 0 exists");
        assert_eq!(first_row.label, "1 · Welcome to Trango Player.");
        assert!(first_row.is_current);
        assert_eq!(window.get_sentence_list_current_index(), 0);

        // When:  reloading with the sample.fi.srt translation fixture merged in
        // Then:  the current cue's translation text is populated, but stays
        //        hidden (show-translation defaults to false)
        let translation_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/sample/sample.fi.srt");
        load_subtitles(
            &window,
            &player_state,
            &subtitle_path,
            Some(&translation_path),
        );
        assert_eq!(
            window.get_translation_text(),
            "Tervetuloa Trango Playeriin."
        );
        assert!(!window.get_show_translation());

        // When:  invoking toggle-translation, as the card's toggle switch does
        // Then:  both the Rust-owned PlayerState and the mirrored Slint
        //        property switch to visible
        window.invoke_toggle_translation();
        assert!(player_state.borrow().show_translation);
        assert!(window.get_show_translation());

        // When:  invoking toggle-translation again
        // Then:  both flip back to hidden
        window.invoke_toggle_translation();
        assert!(!player_state.borrow().show_translation);
        assert!(!window.get_show_translation());

        // When:  wiring the Ctrl+A word-analysis popup (`TODO.md` Vaihe 24)
        //        with a model selected and a cache file already holding an
        //        analysis for the current cue, then requesting it on the
        //        (default) Video source
        // Then:  the popup opens with the cached analysis
        let word_analysis_cache_path = ::word_analysis::cache_path_for(&subtitle_path);
        let mut word_analysis_cache = ::word_analysis::AnalysisCache {
            model: "llama3.1:8b".to_string(),
            entries: std::collections::HashMap::new(),
        };
        word_analysis_cache.entries.insert(
            player_state.borrow().cues[0].index,
            ::word_analysis::WordAnalysis {
                words: vec![::word_analysis::WordEntry {
                    word: "Welcome".to_string(),
                    translation: "Tervetuloa".to_string(),
                    pronunciation: "wel-kuhm".to_string(),
                    parts: Vec::new(),
                }],
            },
        );
        ::word_analysis::save_cache(&word_analysis_cache_path, &word_analysis_cache);

        let word_analysis_current_media = Rc::new(RefCell::new(CurrentMedia {
            media_path: None,
            subtitle_path: Some(subtitle_path.clone()),
            translation_path: None,
        }));
        let word_analysis_selected_model: Rc<RefCell<Option<String>>> =
            Rc::new(RefCell::new(Some("llama3.1:8b".to_string())));
        let word_analysis_target_language = Rc::new(RefCell::new("English".to_string()));
        wire_word_analysis_popup(
            &window,
            &player_state,
            &word_analysis_current_media,
            Rc::clone(&word_analysis_selected_model),
            Rc::clone(&word_analysis_target_language),
            None,
        );

        assert_eq!(player_state.borrow().media_source, MediaSource::Video);
        window.invoke_show_word_analysis();
        assert_eq!(window.get_word_analysis_status(), WordAnalysisStatus::Done);
        let word_analysis_rows = window.get_word_analysis_rows();
        assert_eq!(word_analysis_rows.row_count(), 1);
        assert_eq!(
            word_analysis_rows.row_data(0).expect("row 0 exists").word,
            "Welcome"
        );
        window.invoke_close_word_analysis_popup();
        assert!(!window.get_is_word_analysis_popup_open());

        // When:  the cache holds a blank analysis for the current cue —
        //        what spawn_batch_analyze leaves behind for a cue that
        //        kept failing after all its retries — and Ctrl+A is
        //        pressed on that sentence
        // Then:  it's not treated as a cache hit; the popup goes into
        //        Loading (a fresh Ollama call kicked off) rather than
        //        reopening the same blank result forever
        let mut blank_word_analysis_cache = word_analysis_cache.clone();
        blank_word_analysis_cache.entries.insert(
            player_state.borrow().cues[0].index,
            ::word_analysis::WordAnalysis { words: Vec::new() },
        );
        ::word_analysis::save_cache(&word_analysis_cache_path, &blank_word_analysis_cache);
        window.invoke_show_word_analysis();
        assert_eq!(
            window.get_word_analysis_status(),
            WordAnalysisStatus::Loading
        );
        window.invoke_close_word_analysis_popup();
        ::word_analysis::save_cache(&word_analysis_cache_path, &word_analysis_cache);

        // When:  switching to the Audio source with nothing loaded there
        //        yet (loaded-media-source still Video, forced explicitly
        //        here as a clean baseline regardless of what the earlier
        //        media-ready assertions above left it at)
        // Then:  the sentence card/list blank out — panel_content_ready is
        //        false, so on-select-media-source re-derives the display
        //        from an empty PlayerState instead of leaving the Video
        //        source's sentence stuck on screen — and Ctrl+A reports no
        //        sentence in focus rather than reusing the Video source's
        //        cached analysis
        window.set_loaded_media_source(MediaSourceUi::Video);
        window.invoke_select_media_source(MediaSourceUi::Audio);
        assert_eq!(window.get_sentence_list_rows().row_count(), 0);
        assert_eq!(window.get_sentence_list_current_index(), -1);
        assert_eq!(window.get_sentence_label(), "Sentence – / –");
        assert!(!window.get_has_current_sentence());

        window.invoke_show_word_analysis();
        assert_eq!(window.get_word_analysis_status(), WordAnalysisStatus::Error);
        assert_eq!(
            window.get_word_analysis_error_message(),
            "No sentence is currently in focus."
        );
        window.invoke_close_word_analysis_popup();

        // When:  a matching file loads for the Audio source (mirroring
        //        loaded-media-source the way open_selected_media does) and
        //        the source is re-selected
        // Then:  the real sentence list/card and Ctrl+A's cached analysis
        //        are both back — PlayerState.cues was never touched by any
        //        of the above, only the display
        window.set_loaded_media_source(MediaSourceUi::Audio);
        window.invoke_select_media_source(MediaSourceUi::Audio);
        assert_eq!(window.get_sentence_list_rows().row_count(), 5);
        assert_eq!(window.get_sentence_list_current_index(), 0);

        window.invoke_show_word_analysis();
        assert_eq!(window.get_word_analysis_status(), WordAnalysisStatus::Done);
        let word_analysis_rows = window.get_word_analysis_rows();
        assert_eq!(word_analysis_rows.row_count(), 1);
        assert_eq!(
            word_analysis_rows.row_data(0).expect("row 0 exists").word,
            "Welcome"
        );
        window.invoke_close_word_analysis_popup();
        window.invoke_select_media_source(MediaSourceUi::Video);
        let _ = std::fs::remove_file(&word_analysis_cache_path);

        // When:  wiring the Ctrl+W word-timing popup (`TODO.md` Vaihe 32)
        //        with no whisper model selected yet and requesting it
        // Then:  Error, with the same "select a model first" wording as
        //        the analogous whisper-model guard elsewhere (e.g.
        //        on-generate-subtitles-requested) — no background thread
        //        is spawned, so this is checkable synchronously
        let word_timing_current_media = Rc::new(RefCell::new(CurrentMedia {
            media_path: None,
            subtitle_path: Some(subtitle_path.clone()),
            translation_path: None,
        }));
        let word_timing_selected_model: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        wire_word_timing_popup(
            &window,
            &player_state,
            &word_timing_current_media,
            Rc::clone(&word_timing_selected_model),
            |_command| {},
        );

        window.invoke_show_word_timing();
        assert_eq!(window.get_word_timing_status(), WordTimingStatus::Error);
        assert_eq!(
            window.get_word_timing_error_message(),
            "Select a whisper model first."
        );
        window.invoke_close_word_timing_popup();
        assert!(!window.get_is_word_timing_popup_open());

        // When:  a whisper model is selected but no video/audio file is
        //        open (word_timing_current_media.media_path is still None)
        // Then:  Error, distinct wording from the no-model case
        *word_timing_selected_model.borrow_mut() = Some(PathBuf::from("/models/ggml-base.bin"));
        window.invoke_show_word_timing();
        assert_eq!(window.get_word_timing_status(), WordTimingStatus::Error);
        assert_eq!(
            window.get_word_timing_error_message(),
            "Open a video or audio file first."
        );
        window.invoke_close_word_timing_popup();

        // When:  a model is selected and a media file is open, but no
        //        sentence is in focus (mirrors the identical Ctrl+A check
        //        above: switching to the Audio source with nothing loaded
        //        there yet makes panel_content_ready false)
        // Then:  Error, "No sentence is currently in focus." — same
        //        wording/guard as Ctrl+A's identical scenario
        word_timing_current_media.borrow_mut().media_path =
            Some(PathBuf::from("/videos/some_video.mp4"));
        window.set_loaded_media_source(MediaSourceUi::Video);
        window.invoke_select_media_source(MediaSourceUi::Audio);
        window.invoke_show_word_timing();
        assert_eq!(window.get_word_timing_status(), WordTimingStatus::Error);
        assert_eq!(
            window.get_word_timing_error_message(),
            "No sentence is currently in focus."
        );
        window.invoke_close_word_timing_popup();
        window.invoke_select_media_source(MediaSourceUi::Video);

        // When:  opening the Open dialog with an Up row, a subfolder, and
        //        two video entries
        // Then:  it opens with the folder label mirrored, one row per
        //        entry, and the first *video* row pre-selected (not row 0,
        //        which is the non-selectable Up row)
        let entries = vec![
            open_media_dialog::FolderEntry::Up(PathBuf::from("/")),
            open_media_dialog::FolderEntry::Folder {
                path: PathBuf::from("/videos/clips"),
                name: "clips".to_string(),
            },
            open_media_dialog::FolderEntry::File(open_media_dialog::MediaFileEntry {
                path: PathBuf::from("/videos/a.mp4"),
                name: "a.mp4".to_string(),
                size_label: "10 MB".to_string(),
            }),
            open_media_dialog::FolderEntry::File(open_media_dialog::MediaFileEntry {
                path: PathBuf::from("/videos/b.mkv"),
                name: "b.mkv".to_string(),
                size_label: "20 MB".to_string(),
            }),
        ];
        open_media_dialog::open_dialog(&window, Path::new("/videos"), &entries);
        assert!(window.get_is_open_media_dialog_open());
        assert_eq!(window.get_open_media_folder_label(), "/videos");
        assert_eq!(window.get_open_media_selected_index(), 2);
        let dialog_rows = window.get_open_media_rows();
        assert_eq!(dialog_rows.row_count(), 4);
        assert!(dialog_rows.row_data(0).expect("row 0 exists").is_navigable);
        assert!(dialog_rows.row_data(1).expect("row 1 exists").is_navigable);
        assert!(dialog_rows.row_data(2).expect("row 2 exists").is_selected);
        assert!(!dialog_rows.row_data(3).expect("row 3 exists").is_selected);

        // When:  selecting the second video row, as a row click does
        // Then:  the row model reflects the new selection
        open_media_dialog::mark_selected(&window, &entries, 3);
        let dialog_rows = window.get_open_media_rows();
        assert!(!dialog_rows.row_data(2).expect("row 2 exists").is_selected);
        assert!(dialog_rows.row_data(3).expect("row 3 exists").is_selected);

        // When:  cancelling, as the backdrop/✕/Cancel button does
        // Then:  the dialog closes
        window.set_is_open_media_dialog_open(false);
        assert!(!window.get_is_open_media_dialog_open());

        // When:  wiring the Open Subtitles dialog (Vaihe 19) for a video
        //        whose original subtitle is already tracked as linked, and
        //        requesting it via the top bar's "Subtitles…" button
        // Then:  the modal opens titled after the video, with the original
        //        section linked and the translation section still empty
        //        (no translation tracked yet)
        let sample_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/sample");
        let current_media = Rc::new(RefCell::new(CurrentMedia {
            media_path: Some(sample_dir.join("sample.mp4")),
            subtitle_path: Some(subtitle_path.clone()),
            translation_path: None,
        }));
        let selected_model: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        // wire_open_subtitles_dialog can't be given a real
        // video_player::VideoPlayer here — VideoPlayer::attach needs a
        // real render context that only exists once window.run() is
        // driving the event loop, which this test never does (see this
        // test's own comment on why). reload_calls instead records each
        // call's video_path, standing in for a real VideoPlayer::load_video
        // just well enough to verify wire_open_subtitles_dialog invokes it
        // with the right argument (TODO.md Vaihe 21.6 bugfix below).
        let reload_calls: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
        let reload_calls_for_closure = Rc::clone(&reload_calls);
        wire_open_subtitles_dialog(
            &window,
            &player_state,
            move |_window, video_path, _state| {
                reload_calls_for_closure
                    .borrow_mut()
                    .push(video_path.to_path_buf());
            },
            Rc::clone(&current_media),
            Rc::clone(&selected_model),
        );

        window.invoke_open_subtitles_dialog_requested();
        assert!(window.get_is_open_subtitles_dialog_open());
        assert_eq!(
            window.get_open_subtitles_title(),
            "Subtitles for sample.mp4"
        );
        assert!(window.get_open_subtitles_original_linked());
        assert_eq!(window.get_open_subtitles_original_name(), "sample.srt");
        assert!(!window.get_open_subtitles_translation_linked());

        // When:  requesting the translation-link file picker (replacing
        //        SPEC.md's OS drag-and-drop — see open_subtitles_dialog's
        //        module doc for why)
        // Then:  it lists both .srt files next to the video, sorted by name
        window.invoke_link_translation_requested();
        assert!(window.get_is_link_translation_dialog_open());
        let picker_rows = window.get_link_translation_rows();
        assert_eq!(picker_rows.row_count(), 2);
        assert_eq!(
            picker_rows.row_data(0).expect("row 0 exists").name,
            "sample.fi.srt"
        );
        assert_eq!(
            picker_rows.row_data(1).expect("row 1 exists").name,
            "sample.srt"
        );

        // When:  selecting sample.fi.srt and confirming, as a row
        //        click + the picker's "Link" button do
        // Then:  the picker closes, cues re-merge with the picked
        //        translation, and the Open Subtitles dialog's translation
        //        row reflects the new link
        window.invoke_select_link_translation_row(0);
        window.invoke_confirm_link_translation();
        assert!(!window.get_is_link_translation_dialog_open());
        assert_eq!(
            window.get_translation_text(),
            "Tervetuloa Trango Playeriin."
        );
        assert!(window.get_open_subtitles_translation_linked());
        assert_eq!(
            window.get_open_subtitles_translation_name(),
            "sample.fi.srt"
        );
        assert_eq!(
            current_media.borrow().translation_path,
            Some(sample_dir.join("sample.fi.srt"))
        );

        // When:  closing the Open Subtitles dialog, as footer "Done" does
        // Then:  it closes
        window.invoke_confirm_open_subtitles_dialog();
        assert!(!window.get_is_open_subtitles_dialog_open());

        // When:  switching CurrentMedia to a video with no linked subtitle
        //        (a fake, empty video file in a temp dir — StubSubtitleGenerator
        //        only checks that the video path exists) and requesting the
        //        Open Subtitles dialog again
        // Then:  it opens showing the empty state, generation status Idle
        let generate_dir = std::env::temp_dir().join("trango-test-generate-subtitles-flow");
        let _ = std::fs::remove_dir_all(&generate_dir);
        std::fs::create_dir_all(&generate_dir).expect("failed to create temp test dir");
        let generate_video_path = generate_dir.join("no_subs.mp4");
        std::fs::write(&generate_video_path, b"").expect("failed to write fixture video file");

        *current_media.borrow_mut() = CurrentMedia {
            media_path: Some(generate_video_path.clone()),
            subtitle_path: None,
            translation_path: None,
        };
        window.invoke_open_subtitles_dialog_requested();
        assert!(!window.get_open_subtitles_original_linked());
        assert_eq!(
            window.get_subtitle_generation_status(),
            SubtitleGenerationStatus::Idle
        );

        // When:  generating a subtitle with StubSubtitleGenerator directly
        //        and mirroring it via subtitle_generation::apply_result —
        //        the same UI-thread step the real button's background-
        //        thread callback performs once whisper-cli finishes (see
        //        below), just without the thread hop, since a stub
        //        generator returns instantly
        // Then:  status ends at Done and the dialog's original row reflects
        //        the generated file; loading it into the player and
        //        CurrentMedia the way the button's callback does confirms
        //        the generated .srt is a real, loadable subtitle
        window.set_subtitle_generation_status(SubtitleGenerationStatus::Generating);
        let generated_path = subtitle_generation::apply_result(
            &window,
            subtitle::StubSubtitleGenerator.generate(&generate_video_path),
        );
        assert_eq!(
            window.get_subtitle_generation_status(),
            SubtitleGenerationStatus::Done
        );
        assert!(window.get_open_subtitles_original_linked());
        assert_eq!(window.get_open_subtitles_original_name(), "no_subs.srt");
        assert_eq!(
            generated_path,
            Some(generate_video_path.with_extension("srt"))
        );
        let generated_path = generated_path.expect("stub generator should have produced a path");
        assert!(load_subtitles(
            &window,
            &player_state,
            &generated_path,
            None
        ));
        current_media.borrow_mut().subtitle_path = Some(generated_path.clone());
        assert_eq!(player_state.borrow().cues.len(), 1);
        assert_eq!(
            current_media.borrow().subtitle_path,
            Some(generate_video_path.with_extension("srt"))
        );

        // When:  the AppWindow::subtitle-generated signal fires (as the
        //        real background-thread completion callback's
        //        slint::invoke_from_event_loop closure invokes it, once
        //        back on the UI thread — see wire_open_subtitles_dialog's
        //        doc comment) with the generated path
        // Then:  reload_video is called with the still-open video's path
        //        (TODO.md Vaihe 21.6 bugfix: generation can take long
        //        enough for the video to reach EOF and leave mpv's core
        //        idle, so subsequent cue-navigation seeks need a fresh
        //        loadfile — done by reloading the video here — to recover)
        let subtitle_str = generated_path
            .to_str()
            .expect("fixture path should be valid UTF-8");
        window.invoke_subtitle_generated(subtitle_str.into());
        assert_eq!(
            reload_calls.borrow().as_slice(),
            std::slice::from_ref(&generate_video_path)
        );

        // When:  clicking the real "Generate subtitles" button
        //        (window.invoke_generate_subtitles_requested, wired in
        //        wire_open_subtitles_dialog to subtitle::WhisperCliGenerator
        //        via subtitle_generation::spawn_generate) for a fresh video
        //        with no linked subtitle
        // Then:  it returns immediately with status Generating rather than
        //        blocking the calling thread until a background whisper-cli
        //        process finishes — real transcription can take minutes
        //        (TODO.md Vaihe 21.5), so a click must never freeze the UI
        //        thread. The eventual Done/Error transition is delivered
        //        via slint::invoke_from_event_loop, which needs a running
        //        event loop to process queued closures; this test never
        //        calls AppWindow::run(), so that transition isn't
        //        observable here (see subtitle_generation.rs's own tests
        //        for the background-thread handoff itself)
        let async_dir = std::env::temp_dir().join("trango-test-generate-subtitles-async");
        let _ = std::fs::remove_dir_all(&async_dir);
        std::fs::create_dir_all(&async_dir).expect("failed to create temp test dir");
        let async_video_path = async_dir.join("no_subs_async.mp4");
        std::fs::write(&async_video_path, b"").expect("failed to write fixture video file");
        let fake_model_path = async_dir.join("ggml-fake-model.bin");
        std::fs::write(&fake_model_path, b"").expect("failed to write fixture model file");
        *current_media.borrow_mut() = CurrentMedia {
            media_path: Some(async_video_path),
            subtitle_path: None,
            translation_path: None,
        };

        *selected_model.borrow_mut() = Some(fake_model_path);
        window.invoke_generate_subtitles_requested();
        assert_eq!(
            window.get_subtitle_generation_status(),
            SubtitleGenerationStatus::Generating
        );

        // When:  clicking "Generate subtitles" with no whisper model
        //        selected (TODO.md Vaihe 21.6) — the button is also
        //        disabled in the UI for this state
        //        (whisper-model-selected), this exercises the handler's
        //        defensive fallback directly
        // Then:  status goes straight to Error with a message naming the
        //        actual problem, and no background thread/process is
        //        spawned at all
        *selected_model.borrow_mut() = None;
        window.invoke_generate_subtitles_requested();
        assert_eq!(
            window.get_subtitle_generation_status(),
            SubtitleGenerationStatus::Error
        );
        assert_eq!(
            window.get_subtitle_generation_error_message(),
            "Select a whisper model first."
        );

        std::fs::remove_dir_all(&generate_dir).expect("failed to clean up temp test dir");
        std::fs::remove_dir_all(&async_dir).expect("failed to clean up temp test dir");

        // When:  invoking toggle-audio-capture (Ctrl+Space), wired to a
        //        fake ffmpeg/pactl (real pactl/PulseAudio aren't something
        //        this test environment can rely on)
        // Then:  the first call autodetects the monitor source and starts
        //        a capture, writing to the path AudioCapture::start was
        //        given; the second call stops it, and the file the fake
        //        ffmpeg wrote is left on disk, proving the whole
        //        start/stop -> WAV-file pipeline (TODO.md Vaihe 26/27) end
        //        to end, including the visible filename and its rename
        //        after stopping
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            // The toggle handler persists `audio_recording_folder` to the
            // real config file on every successful start (TODO.md Vaihe
            // 27) — restored via ConfigRestoreGuard so running the test
            // suite doesn't leave the developer's real config pointing at
            // a temp test directory, even if an assertion below panics.
            let original_config = config::load();
            let _config_restore_guard = ConfigRestoreGuard(original_config.clone());

            let audio_capture_dir = std::env::temp_dir().join("trango-test-audio-capture-toggle");
            let _ = std::fs::remove_dir_all(&audio_capture_dir);
            std::fs::create_dir_all(&audio_capture_dir).expect("failed to create temp test dir");
            config::save(&config::TrangoConfig {
                audio_recording_folder: Some(audio_capture_dir.clone()),
                ..original_config.clone()
            });

            let fake_pactl_path = audio_capture_dir.join("fake-pactl.sh");
            std::fs::write(&fake_pactl_path, "#!/bin/sh\necho 'fake-sink'\n")
                .expect("failed to write fake pactl script");
            std::fs::set_permissions(&fake_pactl_path, std::fs::Permissions::from_mode(0o755))
                .expect("failed to make fake pactl executable");

            // Logs its argv, writes a marker to whatever path it was given
            // as its last argument (standing in for the WAV file a real
            // ffmpeg would write), then blocks on stdin until the
            // graceful quit signal arrives.
            let fake_ffmpeg_path = audio_capture_dir.join("fake-ffmpeg.sh");
            std::fs::write(
                &fake_ffmpeg_path,
                format!(
                    "#!/bin/sh\necho \"$@\" > {}/ffmpeg-args.log\nfor last in \"$@\"; do :; done\nprintf 'fake wav content' > \"$last\"\nread -r _line\nexit 0\n",
                    audio_capture_dir.display()
                ),
            )
            .expect("failed to write fake ffmpeg script");
            std::fs::set_permissions(&fake_ffmpeg_path, std::fs::Permissions::from_mode(0o755))
                .expect("failed to make fake ffmpeg executable");

            let mut fake_audio_capture = audio_capture::AudioCapture::default();
            fake_audio_capture.ffmpeg_path = fake_ffmpeg_path;
            fake_audio_capture.pactl_path = fake_pactl_path;
            fake_audio_capture.graceful_stop_timeout = std::time::Duration::from_millis(500);
            // wire_audio_capture can't be given a real
            // video_player::VideoPlayer here, for the same reason
            // wire_open_subtitles_dialog above can't — stopped_calls
            // instead records each call's recording path, standing in for
            // a real open_selected_media just well enough to verify a
            // finished recording is handed off (TODO.md Vaihe 28).
            let stopped_calls: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
            let stopped_calls_for_closure = Rc::clone(&stopped_calls);
            let audio_capture_state = system_audio_capture::wire_audio_capture(
                &window,
                fake_audio_capture,
                move |_window, recording_path| {
                    stopped_calls_for_closure
                        .borrow_mut()
                        .push(recording_path.to_path_buf());
                },
            );

            window.invoke_toggle_audio_capture();
            assert!(audio_capture_state.borrow().is_recording());
            assert_eq!(window.get_audio_capture_error_message(), "");
            assert!(window.get_is_audio_recording());
            let recorded_filename = window.get_audio_recording_filename().to_string();
            assert!(recorded_filename.ends_with(".wav"), "{recorded_filename}");
            let recorded_path = audio_capture_dir.join(&recorded_filename);
            // AudioCapture::start only spawns the ffmpeg (here: fake
            // ffmpeg script) child process and returns immediately, same
            // as it would for a real ffmpeg — the file only appears once
            // that child actually gets scheduled and runs its first
            // write, which under CI load can trail invoke_toggle_audio_capture's
            // return by more than a single poll. Retries briefly instead
            // of asserting immediately to avoid flaking on that race.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !recorded_path.is_file() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(recorded_path.is_file(), "{recorded_path:?}");

            window.invoke_toggle_audio_capture();
            assert!(!audio_capture_state.borrow().is_recording());
            assert_eq!(*stopped_calls.borrow(), vec![recorded_path.clone()]);
            assert_eq!(window.get_audio_capture_error_message(), "");
            assert!(!window.get_is_audio_recording());

            // The recording this capture wrote should still be on disk
            // (TODO.md Vaihe 26 records to a single file, not a temp
            // buffer that gets cleaned up), under the timestamped default
            // filename shown while recording.
            assert_eq!(
                std::fs::read_to_string(&recorded_path).unwrap(),
                "fake wav content"
            );

            // When:  the filename field is edited (Enter) after the
            //        recording has stopped
            // Then:  the file on disk is renamed to match, and the
            //        displayed filename reflects the new name
            window.invoke_rename_audio_recording_file("der_anruf.wav".into());
            assert_eq!(window.get_audio_recording_filename(), "der_anruf.wav");
            assert!(!recorded_path.exists());
            assert_eq!(
                std::fs::read_to_string(audio_capture_dir.join("der_anruf.wav")).unwrap(),
                "fake wav content"
            );

            std::fs::remove_dir_all(&audio_capture_dir).expect("failed to clean up temp test dir");

            // When:  toggle-audio-capture is wired to an AudioCapture whose
            //        ffmpeg_path names a binary that doesn't exist (fake
            //        pactl still autodetects fine)
            // Then:  audio-capture-error-message surfaces the "ffmpeg not
            //        found" explanation instead of only logging it, so a
            //        broken install doesn't look like Ctrl+Space did
            //        nothing
            let broken_dir = std::env::temp_dir().join("trango-test-audio-capture-error");
            let _ = std::fs::remove_dir_all(&broken_dir);
            std::fs::create_dir_all(&broken_dir).expect("failed to create temp test dir");
            // Points audio_recording_folder at broken_dir (which exists),
            // so this test's "ffmpeg not found" assertion below can't be
            // preempted by the folder-doesn't-exist check added ahead of
            // it in wire_audio_capture's toggle handler.
            config::save(&config::TrangoConfig {
                audio_recording_folder: Some(broken_dir.clone()),
                ..original_config.clone()
            });
            let fake_pactl_path = broken_dir.join("fake-pactl.sh");
            std::fs::write(&fake_pactl_path, "#!/bin/sh\necho 'fake-sink'\n")
                .expect("failed to write fake pactl script");
            std::fs::set_permissions(&fake_pactl_path, std::fs::Permissions::from_mode(0o755))
                .expect("failed to make fake pactl executable");
            let mut broken_audio_capture = audio_capture::AudioCapture::default();
            broken_audio_capture.ffmpeg_path = broken_dir.join("no-such-ffmpeg-binary");
            broken_audio_capture.pactl_path = fake_pactl_path;
            let broken_audio_capture_state = system_audio_capture::wire_audio_capture(
                &window,
                broken_audio_capture,
                |_window, _recording_path| {},
            );

            window.invoke_toggle_audio_capture();
            assert!(!broken_audio_capture_state.borrow().is_recording());
            assert!(window
                .get_audio_capture_error_message()
                .contains("ffmpeg not found"));

            std::fs::remove_dir_all(&broken_dir).expect("failed to clean up temp test dir");

            // When:  toggle-audio-capture is invoked with
            //        audio_recording_folder pointing at a folder that
            //        doesn't exist
            // Then:  audio-capture-error-message explains the folder is
            //        missing, rather than silently doing nothing —
            //        ffmpeg's own stderr is discarded
            //        (AudioCapture::start's Stdio::null()), so without this
            //        check the failure wouldn't surface anywhere the user
            //        could see it
            let missing_recording_folder =
                std::env::temp_dir().join("trango-test-audio-capture-missing-folder");
            let _ = std::fs::remove_dir_all(&missing_recording_folder);
            config::save(&config::TrangoConfig {
                audio_recording_folder: Some(missing_recording_folder.clone()),
                ..original_config.clone()
            });

            let missing_folder_dir =
                std::env::temp_dir().join("trango-test-audio-capture-missing-folder-setup");
            let _ = std::fs::remove_dir_all(&missing_folder_dir);
            std::fs::create_dir_all(&missing_folder_dir).expect("failed to create temp test dir");
            let fake_pactl_path = missing_folder_dir.join("fake-pactl.sh");
            std::fs::write(&fake_pactl_path, "#!/bin/sh\necho 'fake-sink'\n")
                .expect("failed to write fake pactl script");
            std::fs::set_permissions(&fake_pactl_path, std::fs::Permissions::from_mode(0o755))
                .expect("failed to make fake pactl executable");
            let mut missing_folder_capture = audio_capture::AudioCapture::default();
            missing_folder_capture.pactl_path = fake_pactl_path;
            let missing_folder_state = system_audio_capture::wire_audio_capture(
                &window,
                missing_folder_capture,
                |_window, _recording_path| {},
            );

            assert_eq!(
                window.get_audio_recording_folder_label(),
                missing_recording_folder.display().to_string()
            );

            window.invoke_toggle_audio_capture();
            assert!(!missing_folder_state.borrow().is_recording());
            assert!(!window.get_is_audio_recording());
            assert!(window
                .get_audio_capture_error_message()
                .contains("Recording folder does not exist"));

            std::fs::remove_dir_all(&missing_folder_dir).expect("failed to clean up temp test dir");
        }

        // When:  the Settings dialog (top bar's gear icon) is opened with a
        //        known config.toml
        // Then:  its display properties mirror that config, and editing the
        //        audio-monitor-source/audio-recording-folder fields
        //        persists immediately (same as wire_ollama_target_language)
        //        and updates the Audio panel's "Saving to:" label
        {
            let original_config = config::load();
            let _config_restore_guard = ConfigRestoreGuard(original_config.clone());

            let settings_dir = std::env::temp_dir().join("trango-test-settings-dialog");
            let _ = std::fs::remove_dir_all(&settings_dir);
            std::fs::create_dir_all(&settings_dir).expect("failed to create temp test dir");
            config::save(&config::TrangoConfig {
                video_folder: Some(settings_dir.join("videos")),
                audio_monitor_source: Some("alsa_output.analog-stereo.monitor".to_string()),
                audio_recording_folder: Some(settings_dir.clone()),
                ..config::TrangoConfig::default()
            });

            wire_settings_dialog(&window);

            window.invoke_settings_dialog_requested();
            assert!(window.get_is_settings_dialog_open());
            assert!(window.get_settings_config_path().contains("config.toml"));
            assert_eq!(
                window.get_settings_video_folder(),
                settings_dir.join("videos").display().to_string()
            );
            assert_eq!(
                window.get_settings_audio_monitor_source(),
                "alsa_output.analog-stereo.monitor"
            );
            assert_eq!(
                window.get_settings_audio_recording_folder(),
                settings_dir.display().to_string()
            );
            assert!(window.get_settings_audio_recording_folder_exists());

            window.invoke_close_settings_dialog();
            assert!(!window.get_is_settings_dialog_open());

            let new_video_folder = settings_dir.join("other-videos");
            window.invoke_set_settings_video_folder(new_video_folder.display().to_string().into());
            assert_eq!(config::load().video_folder, Some(new_video_folder.clone()));
            assert_eq!(
                window.get_settings_video_folder(),
                new_video_folder.display().to_string()
            );

            window.invoke_set_settings_audio_monitor_source("custom.monitor".into());
            assert_eq!(
                config::load().audio_monitor_source,
                Some("custom.monitor".to_string())
            );

            let missing_folder = settings_dir.join("does-not-exist-yet");
            window.invoke_set_settings_audio_recording_folder(
                missing_folder.display().to_string().into(),
            );
            assert_eq!(
                config::load().audio_recording_folder,
                Some(missing_folder.clone())
            );
            assert!(!window.get_settings_audio_recording_folder_exists());
            assert_eq!(
                window.get_audio_recording_folder_label(),
                missing_folder.display().to_string()
            );

            std::fs::remove_dir_all(&settings_dir).expect("failed to clean up temp test dir");
        }
    }
}
