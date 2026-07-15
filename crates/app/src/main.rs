//! TrangoPlayer entry point.
//!
//! Initializes logging, then opens the Slint main window (see
//! `ui/app-window.slint`). libmpv integration and the rest of the UI are
//! wired in later development steps (see `TODO.md`).

mod open_video_dialog;
mod sentence_card;
mod sentence_list;
mod video_player;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use playback_state::{PlaybackMode, PlayerState, SeekCommand};

slint::include_modules!();

/// Prints the current crate version to stdout.
fn print_version() {
    println!("trango {}", env!("CARGO_PKG_VERSION"));
}

/// Owns a fresh `PlayerState` — defaulting to `SentenceBySentence` mode, the
/// primary language-learning use case — and mirrors that default into the
/// window's `sentence-mode-active` property, since `app-window.slint`
/// itself only hardcodes `false`. Also wires the window's `toggle-mode`
/// callback (invoked by the top bar's segmented control) to
/// `PlayerState::toggle_mode()`, mirroring each subsequent mode change back
/// into `sentence-mode-active` too, and the window's `toggle-translation`
/// callback (invoked by the current-sentence card's toggle switch) to
/// `PlayerState::toggle_translation()`, mirroring `show_translation` into
/// the window's `show-translation` property the same way. Returns the
/// shared state so callers can inspect it (used by tests; later steps will
/// read it too).
fn wire_player_state(window: &AppWindow) -> Rc<RefCell<PlayerState>> {
    let state = Rc::new(RefCell::new(PlayerState::new()));
    window.set_sentence_mode_active(state.borrow().mode == PlaybackMode::SentenceBySentence);
    window.set_show_translation(state.borrow().show_translation);

    let state_for_callback = Rc::clone(&state);
    let window_weak = window.as_weak();
    window.on_toggle_mode(move || {
        let mode = {
            let mut state = state_for_callback.borrow_mut();
            state.toggle_mode();
            state.mode
        };
        tracing::debug!(?mode, "playback mode toggled");
        if let Some(window) = window_weak.upgrade() {
            window.set_sentence_mode_active(mode == PlaybackMode::SentenceBySentence);
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
/// handler for Right/Left/Space while in `SentenceBySentence` mode, and by
/// the sentence list's row clicks, respectively — to `PlayerState`'s
/// matching navigation methods, mirroring the resulting cue into the
/// sentence card/list and handing any produced `SeekCommand` to
/// `video_player` to drive mpv (seek + play-to-end + pause, see
/// `video_player::VideoPlayer::apply_seek_command`).
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
    window.on_repeat_cue(cue_navigation_handler(
        window,
        state,
        &video_player,
        |state| state.repeat_current_cue(),
    ));

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
/// mpv. Shared by arrow/space key handling and the sentence list's row-click
/// handling so both paths behave identically, per README's "Sentence list"
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
        video_player.apply_seek_command(command);
    }
}

/// Reads the video path to play (if any) from CLI arguments, as used by
/// `main`. `args` is expected to include the program name at index 0 (i.e.
/// `std::env::args()`), matching Vaihe 11's `trango <path/to/video>` usage.
/// A video can also be picked in-app via the top bar's "Open video…" button
/// (see `wire_open_video_dialog`) instead of a CLI argument.
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
/// sentence card and sentence list. Leaves `state` untouched if
/// `subtitle_path` can't be read or parsed, since a bad subtitle path
/// shouldn't prevent the video from playing. A `translation_path` that
/// can't be read or parsed is logged and simply skipped — the original
/// cues still load, just without translations.
fn load_subtitles(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    subtitle_path: &Path,
    translation_path: Option<&Path>,
) {
    let Some(mut cues) = parse_subtitle_file(subtitle_path) else {
        return;
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
}

/// Resolves the folder the Open Video dialog lists by default: the CLI
/// video path's parent directory if one was given (Vaihe 11's
/// `trango <path/to/video>` usage — likely where the user keeps other
/// videos too), otherwise the current working directory. An in-dialog
/// folder switcher is out of scope for Vaihe 18 — see
/// `docs/src/specs/README.md`.
fn default_video_folder(args: &[String]) -> PathBuf {
    video_path_from_args(args)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|err| {
                tracing::warn!(%err, "failed to read current directory; falling back to \".\"");
                PathBuf::from(".")
            })
        })
}

/// Wires the Open Video dialog (Vaihe 18): the top bar's
/// `open-video-dialog-requested` callback lists `default_folder`'s entries
/// and opens the modal (`open_video_dialog::open_dialog`);
/// `select-open-video-row` either navigates to a different folder (an
/// `Up`/`Folder` row — re-listing and re-populating the dialog in place) or
/// marks a video row selected (`open_video_dialog::mark_selected`);
/// `confirm-open-video` loads the selected video (see
/// `open_selected_video`); `cancel-open-video-dialog` (backdrop/✕/Cancel)
/// just closes it.
fn wire_open_video_dialog(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    video_player_slot: Rc<RefCell<Option<Rc<video_player::VideoPlayer>>>>,
    default_folder: PathBuf,
) {
    let entries: Rc<RefCell<Vec<open_video_dialog::FolderEntry>>> =
        Rc::new(RefCell::new(Vec::new()));

    let request_window_weak = window.as_weak();
    let request_entries = Rc::clone(&entries);
    window.on_open_video_dialog_requested(move || {
        let Some(window) = request_window_weak.upgrade() else {
            return;
        };
        let files = open_video_dialog::list_folder_entries(&default_folder);
        open_video_dialog::open_dialog(&window, &default_folder, &files);
        *request_entries.borrow_mut() = files;
    });

    let cancel_window_weak = window.as_weak();
    window.on_cancel_open_video_dialog(move || {
        if let Some(window) = cancel_window_weak.upgrade() {
            window.set_is_open_video_dialog_open(false);
        }
    });

    let select_window_weak = window.as_weak();
    let select_entries = Rc::clone(&entries);
    window.on_select_open_video_row(move |index| {
        let Some(window) = select_window_weak.upgrade() else {
            return;
        };
        let target_folder = usize::try_from(index).ok().and_then(|index| {
            match select_entries.borrow().get(index)? {
                open_video_dialog::FolderEntry::Up(path)
                | open_video_dialog::FolderEntry::Folder { path, .. } => Some(path.clone()),
                open_video_dialog::FolderEntry::Video(_) => None,
            }
        });
        if let Some(target_folder) = target_folder {
            let files = open_video_dialog::list_folder_entries(&target_folder);
            open_video_dialog::open_dialog(&window, &target_folder, &files);
            *select_entries.borrow_mut() = files;
            return;
        }
        window.set_open_video_selected_index(index);
        open_video_dialog::mark_selected(&window, &select_entries.borrow(), index);
    });

    let confirm_window_weak = window.as_weak();
    let confirm_entries = Rc::clone(&entries);
    let confirm_state = Rc::clone(state);
    window.on_confirm_open_video(move || {
        let Some(window) = confirm_window_weak.upgrade() else {
            return;
        };
        let video_path = usize::try_from(window.get_open_video_selected_index())
            .ok()
            .and_then(|index| confirm_entries.borrow().get(index).cloned())
            .and_then(|entry| match entry {
                open_video_dialog::FolderEntry::Video(video) => Some(video.path),
                _ => None,
            });
        let Some(video_path) = video_path else {
            return;
        };
        window.set_is_open_video_dialog_open(false);
        open_selected_video(&window, &confirm_state, &video_player_slot, &video_path);
    });
}

/// Loads `video_path` into playback — reusing `video_player_slot`'s
/// `VideoPlayer` if one is already attached (switching files mid-session),
/// or attaching a fresh one (and wiring cue navigation for it) if this is
/// the session's first video, e.g. when trango was started without a CLI
/// video argument. Resolves a same-stem `.srt` first
/// (`open_video_dialog::matching_subtitle_path`) and loads it via
/// `load_subtitles` if found, clearing any previously loaded cues
/// otherwise — done before attaching/loading the video so that, in
/// `SentenceBySentence` mode, the start-of-playback pause lands on the new
/// video's first cue rather than a stale one from a previously opened
/// video. Called by `wire_open_video_dialog`'s `confirm-open-video`
/// handler.
fn open_selected_video(
    window: &AppWindow,
    state: &Rc<RefCell<PlayerState>>,
    video_player_slot: &Rc<RefCell<Option<Rc<video_player::VideoPlayer>>>>,
    video_path: &Path,
) {
    match open_video_dialog::matching_subtitle_path(video_path) {
        Some(subtitle_path) => {
            tracing::info!(
                ?subtitle_path,
                "auto-matched subtitle file for opened video"
            );
            load_subtitles(window, state, &subtitle_path, None);
        }
        None => {
            state.borrow_mut().set_cues(Vec::new());
            sentence_card::update_sentence_card(window, &state.borrow());
            sentence_list::update_sentence_list(window, &state.borrow());
        }
    }

    let existing_player = video_player_slot.borrow().clone();
    match existing_player {
        Some(video_player) => video_player.load_video(video_path, &state.borrow()),
        None => match video_player::VideoPlayer::attach(window, video_path, Rc::clone(state)) {
            Ok(video_player) => {
                let video_player = Rc::new(video_player);
                wire_cue_navigation(window, state, Rc::clone(&video_player));
                *video_player_slot.borrow_mut() = Some(video_player);
            }
            Err(err) => tracing::error!(%err, ?video_path, "failed to attach video player"),
        },
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("trango starting");
    print_version();

    let window = AppWindow::new()?;
    window.set_version(env!("CARGO_PKG_VERSION").into());
    let player_state = wire_player_state(&window);
    sentence_card::update_sentence_card(&window, &player_state.borrow());
    sentence_list::update_sentence_list(&window, &player_state.borrow());

    let args: Vec<String> = std::env::args().collect();
    if let Some(subtitle_path) = subtitle_path_from_args(&args) {
        let translation_path = translation_path_from_args(&args);
        load_subtitles(
            &window,
            &player_state,
            &subtitle_path,
            translation_path.as_deref(),
        );
    }

    let video_player_slot: Rc<RefCell<Option<Rc<video_player::VideoPlayer>>>> =
        Rc::new(RefCell::new(None));
    if let Some(video_path) = video_path_from_args(&args) {
        let video_player = Rc::new(video_player::VideoPlayer::attach(
            &window,
            &video_path,
            Rc::clone(&player_state),
        )?);
        wire_cue_navigation(&window, &player_state, Rc::clone(&video_player));
        *video_player_slot.borrow_mut() = Some(video_player);
    } else {
        tracing::info!(
            "no video path given; use the \"Open video…\" button or run as `trango <path/to/video>`"
        );
    }

    wire_open_video_dialog(
        &window,
        &player_state,
        Rc::clone(&video_player_slot),
        default_video_folder(&args),
    );

    window.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use slint::Model;

    use super::*;

    #[test]
    fn test_version_is_set() {
        // Given: the crate's compiled version metadata
        // When:  reading CARGO_PKG_VERSION
        // Then:  it is non-empty, proving the version is wired up for display
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
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
        // Given: argv with a video path that has a parent directory
        let args = vec!["trango".to_string(), "some/folder/video.mp4".to_string()];

        // When:  resolving the default Open Video dialog folder
        // Then:  it's that video's parent directory
        assert_eq!(default_video_folder(&args), PathBuf::from("some/folder"));
    }

    #[test]
    fn test_default_video_folder_without_cli_video_path() {
        // Given: argv with no video path
        let args = vec!["trango".to_string()];

        // When:  resolving the default Open Video dialog folder
        // Then:  it falls back to the current working directory
        assert_eq!(
            default_video_folder(&args),
            std::env::current_dir().expect("failed to read current directory")
        );
    }

    #[test]
    fn test_default_video_folder_with_bare_filename_falls_back_to_cwd() {
        // Given: argv with a video path that has no parent directory
        //        component (a bare filename)
        let args = vec!["trango".to_string(), "video.mp4".to_string()];

        // When:  resolving the default Open Video dialog folder
        // Then:  it falls back to the current working directory, since
        //        "video.mp4"'s parent is the empty path, not a real folder
        assert_eq!(
            default_video_folder(&args),
            std::env::current_dir().expect("failed to read current directory")
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

        // When:  reading sentence_mode_active before wiring
        // Then:  it's still app-window.slint's own hardcoded default (false)
        assert!(!window.get_sentence_mode_active());

        // When:  wiring a fresh PlayerState
        // Then:  it defaults to SentenceBySentence (the primary language-
        //        learning use case), mirrored into sentence_mode_active
        let player_state = wire_player_state(&window);
        assert_eq!(player_state.borrow().mode, PlaybackMode::SentenceBySentence);
        assert!(window.get_sentence_mode_active());

        // When:  invoking toggle-mode, as a segmented control click does
        // Then:  both the Rust-owned PlayerState and the mirrored Slint
        //        property switch to Normal
        window.invoke_toggle_mode();
        assert_eq!(player_state.borrow().mode, PlaybackMode::Normal);
        assert!(!window.get_sentence_mode_active());

        // When:  invoking toggle-mode again
        // Then:  both flip back to SentenceBySentence
        window.invoke_toggle_mode();
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

        // When:  opening the Open Video dialog with an Up row, a subfolder,
        //        and two video entries
        // Then:  it opens with the folder label mirrored, one row per
        //        entry, and the first *video* row pre-selected (not row 0,
        //        which is the non-selectable Up row)
        let entries = vec![
            open_video_dialog::FolderEntry::Up(PathBuf::from("/")),
            open_video_dialog::FolderEntry::Folder {
                path: PathBuf::from("/videos/clips"),
                name: "clips".to_string(),
            },
            open_video_dialog::FolderEntry::Video(open_video_dialog::VideoFileEntry {
                path: PathBuf::from("/videos/a.mp4"),
                name: "a.mp4".to_string(),
                size_label: "10 MB".to_string(),
            }),
            open_video_dialog::FolderEntry::Video(open_video_dialog::VideoFileEntry {
                path: PathBuf::from("/videos/b.mkv"),
                name: "b.mkv".to_string(),
                size_label: "20 MB".to_string(),
            }),
        ];
        open_video_dialog::open_dialog(&window, Path::new("/videos"), &entries);
        assert!(window.get_is_open_video_dialog_open());
        assert_eq!(window.get_open_video_folder_label(), "/videos");
        assert_eq!(window.get_open_video_selected_index(), 2);
        let dialog_rows = window.get_open_video_rows();
        assert_eq!(dialog_rows.row_count(), 4);
        assert!(dialog_rows.row_data(0).expect("row 0 exists").is_navigable);
        assert!(dialog_rows.row_data(1).expect("row 1 exists").is_navigable);
        assert!(dialog_rows.row_data(2).expect("row 2 exists").is_selected);
        assert!(!dialog_rows.row_data(3).expect("row 3 exists").is_selected);

        // When:  selecting the second video row, as a row click does
        // Then:  the row model reflects the new selection
        open_video_dialog::mark_selected(&window, &entries, 3);
        let dialog_rows = window.get_open_video_rows();
        assert!(!dialog_rows.row_data(2).expect("row 2 exists").is_selected);
        assert!(dialog_rows.row_data(3).expect("row 3 exists").is_selected);

        // When:  cancelling, as the backdrop/✕/Cancel button does
        // Then:  the dialog closes
        window.set_is_open_video_dialog_open(false);
        assert!(!window.get_is_open_video_dialog_open());
    }
}
