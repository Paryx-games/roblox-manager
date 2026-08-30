// The release binary is a GUI app with no console. Under `cargo test` the same
// attribute would detach the test harness from its console and swallow every
// result, so it is applied only to non-test builds.
#![cfg_attr(all(not(test), not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod bridge;
mod browser_login;
mod components;
mod icons;
mod startup;
mod theme;
mod toast;

use std::path::PathBuf;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::EnvFilter;

// ---------------------------------------------------------------------------
// Log file
// ---------------------------------------------------------------------------

/// Rotated log files kept on disk, oldest deleted first. One file per day, so
/// this is roughly a week of history.
const LOG_FILES_KEPT: usize = 7;

/// Open the rotating log file under `data_dir`, or `None` if it cannot be
/// created.
///
/// Files are named `rm.<YYYY-MM-DD>.log`. Before this the app appended forever
/// to a single `rm.log`, which nothing ever truncated while the presence timer
/// logged at `info` on a recurring tick.
///
/// Deliberately **not** wrapped in `tracing_appender::non_blocking`: that hands
/// writes to a worker thread and relies on a `WorkerGuard` dropping at the end
/// of `main` to flush, which a panic or an abort can skip. Writing
/// synchronously costs a syscall per event at a volume of a few events a
/// minute, and in exchange the last line before a crash is always on disk.
fn log_appender(
    data_dir: &std::path::Path,
) -> Option<tracing_appender::rolling::RollingFileAppender> {
    tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("rm")
        .filename_suffix("log")
        .max_log_files(LOG_FILES_KEPT)
        .build(data_dir)
        .ok()
}

// ---------------------------------------------------------------------------
// Log scrubbing
// ---------------------------------------------------------------------------

/// Wraps a `MakeWriter` so every formatted event passes through
/// [`ram_core::redact::scrub`] on its way to the file.
///
/// This sits *below* every call site on purpose. Redacting at the one known
/// offender (the launch URI) fixes today's leak; redacting here is what stops
/// the next `debug!("{cookie}")` from quietly reintroducing it. It is a
/// backstop, not permission to log secrets.
struct Scrubbed<M>(M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Scrubbed<M> {
    type Writer = ScrubbingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        ScrubbingWriter::new(self.0.make_writer())
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        ScrubbingWriter::new(self.0.make_writer_for(meta))
    }
}

/// Buffers one event's bytes, scrubs them, then forwards them in a single
/// write.
///
/// Buffering rather than scrubbing each chunk matters: the formatter is free to
/// split a line across several `write` calls, and a secret straddling that
/// boundary would slip past a per-chunk regex. `tracing-subscriber`'s fmt layer
/// creates one writer per event and drops it immediately afterwards, so `Drop`
/// is the flush point.
struct ScrubbingWriter<W: std::io::Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: std::io::Write> ScrubbingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            buf: Vec::new(),
        }
    }

    fn emit(&mut self) -> std::io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let taken = std::mem::take(&mut self.buf);
        let text = String::from_utf8_lossy(&taken);
        self.inner
            .write_all(ram_core::redact::scrub(&text).as_bytes())?;
        self.inner.flush()
    }
}

impl<W: std::io::Write> std::io::Write for ScrubbingWriter<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.emit()
    }
}

impl<W: std::io::Write> Drop for ScrubbingWriter<W> {
    fn drop(&mut self) {
        // The event is only on disk once this runs, so a lost error here would
        // be a lost log line. Nothing useful can be done about it at this point
        // (the log is the reporting channel), so it is dropped deliberately.
        let _ = self.emit();
    }
}

/// Marker recording that the pre-rotation log has already been dealt with.
const LEGACY_LOG_PURGED_MARKER: &str = ".rm.log.purged";

/// Delete the pre-rotation `rm.log`, once.
///
/// Before daily rotation existed, everything appended forever to a single
/// `rm.log`. Two problems with leaving it lying around. It is unbounded, and it
/// predates the scrubbing writer, so on at least one machine it accumulated
/// 1,593 INFO lines each containing a full Roblox launch URI, `gameinfo:`
/// authentication ticket and all. Those tickets are long expired, but the file
/// is exactly the kind of thing a user attaches to a bug report, and nothing
/// will ever rewrite it because nothing writes to that name any more.
///
/// Only `rm.log` is touched. The rotating files are `rm.<date>.log` and are
/// live history that the user may still need.
///
/// The marker file is what makes this once rather than every start. Deleting
/// the log alone would be nearly idempotent, but a user who deliberately
/// recreates `rm.log` (say, to pin an old session while debugging) should not
/// have RM quietly eat it on every launch.
fn purge_legacy_log(data_dir: &std::path::Path) {
    let marker = data_dir.join(LEGACY_LOG_PURGED_MARKER);
    if marker.exists() {
        return;
    }
    let legacy = data_dir.join("rm.log");
    if legacy.is_file() {
        match std::fs::remove_file(&legacy) {
            Ok(()) => tracing::info!("Removed the pre-rotation rm.log"),
            Err(e) => {
                // Most likely another RM still has it open. Leave the marker
                // unwritten so the next start tries again.
                tracing::warn!("Could not remove the pre-rotation rm.log: {e}");
                return;
            }
        }
    }
    // Written whether or not the file existed: a fresh install has nothing to
    // purge and should not keep checking. Through `atomic_swap` rather than a
    // bare `fs::write` because that is the house rule for anything that
    // persists across runs, even a zero-byte sentinel: a half-created marker
    // would be indistinguishable from a real one.
    let _ = ram_core::storage::atomic_swap(&marker, b"");
}

/// Canonical data directory: `%APPDATA%\RM`.
pub fn data_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("RM")
}

fn startup_log_level(data_dir: &std::path::Path) -> ram_core::models::LogLevel {
    let legacy = PathBuf::from("config.json");
    let config_path = if legacy.is_file() && !data_dir.join("config.json").is_file() {
        legacy
    } else {
        data_dir.join("config.json")
    };
    let mut config = ram_core::AppConfig::load(&config_path);
    config.log_level = config.log_level.clamp_for_profile();
    config.log_level
}

/// One-time migration: turn each entry of `config.favorite_places` into a
/// standalone preset file under `presets/`, then clear the list in config.
/// Runs silently and is a no-op when there's nothing to migrate.
fn maybe_migrate_favorites(data_dir: &std::path::Path) {
    let config_path = data_dir.join("config.json");
    if !config_path.is_file() {
        return;
    }
    let mut config = ram_core::AppConfig::load(&config_path);
    if config.favorite_places.is_empty() {
        return;
    }
    let mut migrated = 0;
    for fav in config.favorite_places.drain(..) {
        let preset = ram_core::models::LaunchPreset {
            name: fav.name,
            place_id: fav.place_id,
            job_id: None,
            data: None,
        };
        if ram_core::presets::save(data_dir, &preset, None).is_ok() {
            migrated += 1;
        }
    }
    if migrated > 0 {
        let _ = config.save(&config_path);
        tracing::info!("Migrated {migrated} legacy favorite(s) into preset files");
    }
}

/// Check for legacy data files next to the exe and offer to migrate them.
fn maybe_migrate_legacy_data(data_dir: &std::path::Path) {
    let legacy_config = PathBuf::from("config.json");
    let legacy_accounts = PathBuf::from("accounts.dat");

    let has_legacy = legacy_config.is_file() || legacy_accounts.is_file();
    let has_new = data_dir.join("config.json").is_file();

    if !has_legacy || has_new {
        return;
    }

    // Show a native dialog before the egui window opens
    let result = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title("RM - Migrate Data")
        .set_description(
            "RM now stores data in a standard location so it works \
             no matter where the exe is placed.\n\n\
             Found existing data next to the exe. Move it to the new location?\n\n\
             • Yes: move files (recommended)\n\
             • No: keep using files next to the exe",
        )
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();

    if result == rfd::MessageDialogResult::Yes {
        if let Err(e) = std::fs::create_dir_all(data_dir) {
            tracing::error!("Failed to create data dir: {e}");
            return;
        }
        for name in &["config.json", "accounts.dat"] {
            let src = PathBuf::from(name);
            if src.is_file() {
                let dst = data_dir.join(name);
                if let Err(e) = std::fs::rename(&src, &dst) {
                    // rename can fail across volumes; fall back to copy+delete
                    if let Err(e2) = std::fs::copy(&src, &dst) {
                        tracing::error!("Failed to migrate {name}: rename={e}, copy={e2}");
                    } else {
                        let _ = std::fs::remove_file(&src);
                    }
                }
            }
        }
    }
}

fn main() {
    let data_dir = data_dir();
    let _ = std::fs::create_dir_all(&data_dir);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(startup_log_level(&data_dir).filter_string()));
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter);

    // Debug builds keep a console attached so `cargo run` shows the same
    // startup and runtime diagnostics that are written to the release log.
    if cfg!(debug_assertions) {
        subscriber.with_writer(std::io::stderr).init();
    } else {
        // Release builds have no console, so write to the rotating log file.
        match log_appender(&data_dir) {
            Some(appender) => subscriber.with_writer(Scrubbed(appender)).init(),
            None => subscriber.init(),
        }
    }

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        profile = if cfg!(debug_assertions) { "debug" } else { "release" },
        data_dir = %data_dir.display(),
        "RM started"
    );

    // Install a panic hook that flushes the message to the log before dying.
    // Nothing extra is needed to make that flush happen: the appender writes
    // straight to the file on every event (no `tracing_appender::non_blocking`
    // worker thread), so by the time `error!` returns the bytes are already
    // out. Introducing the non-blocking writer here would mean a panic could
    // race its own log line to the exit.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("PANIC: {info}");
        prev_hook(info);
    }));

    // Browser-login child mode: re-entry point when the parent UI spawns us
    // with the browser_login flag. Hosts the webview on this process's main
    // thread and exits when the cookie is captured or the user cancels.
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == browser_login::FLAG {
        let profile_dir = PathBuf::from(&args[2]);
        let outfile = PathBuf::from(&args[3]);
        let code = browser_login::run_child(profile_dir, outfile);
        std::process::exit(code);
    }
    // "Open browser as" child mode — same re-exec trick, but pre-loaded with
    // an account's cookie and left open until the user closes the window.
    if args.len() >= 4 && args[1] == browser_login::BROWSE_AS_FLAG {
        let profile_dir = PathBuf::from(&args[2]);
        let cookie_in = PathBuf::from(&args[3]);
        let label = args.get(4).cloned().unwrap_or_default();
        let code = if let Some(destination_url) = args.get(5) {
            browser_login::run_browse_as_child_to(
                profile_dir,
                cookie_in,
                label,
                destination_url.clone(),
            )
        } else {
            browser_login::run_browse_as_child(profile_dir, cookie_in, label)
        };
        std::process::exit(code);
    }

    // Get rid of the pre-rotation log, which predates the scrubbing writer and
    // can still be holding launch URIs. After the subscriber is installed so
    // the outcome is logged, and before anything else runs.
    purge_legacy_log(&data_dir);

    // Offer to migrate legacy data from the exe directory
    maybe_migrate_legacy_data(&data_dir);

    // Migrate legacy `favorite_places` (inline in config.json) into the new
    // per-file preset system. Runs once: when the migration succeeds we clear
    // the config field so subsequent startups skip the loop.
    maybe_migrate_favorites(&data_dir);

    // Resolve config and account paths.
    // If a legacy config.json still exists next to the exe (user declined migration),
    // keep using local paths for backwards compatibility.
    let (config_path, config) =
        if PathBuf::from("config.json").is_file() && !data_dir.join("config.json").is_file() {
            // User declined migration — use local files
            let p = PathBuf::from("config.json");
            let c = ram_core::AppConfig::load(&p);
            (p, c)
        } else {
            let p = data_dir.join("config.json");
            let mut c = ram_core::AppConfig::load(&p);
            // Ensure accounts_path is absolute under the data dir
            if c.accounts_path == std::path::Path::new("accounts.dat") {
                c.accounts_path = data_dir.join("accounts.dat");
            }
            (p, c)
        };

    // Decode the embedded logo for the window icon.
    let icon = {
        let png = include_bytes!("../../assets/Logo.png");
        let img = image::load_from_memory(png).expect("failed to decode Logo.png");
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        eframe::egui::IconData {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        }
    };

    let window_width = config.window_width.max(1080.0);
    let window_height = config.window_height.max(600.0);
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([window_width, window_height])
            .with_min_inner_size([1080.0, 600.0])
            .with_title(format!(
                "RM | Roblox Manager v{}",
                env!("CARGO_PKG_VERSION")
            ))
            .with_icon(icon),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "RM",
        native_options,
        Box::new(move |cc| {
            // Enable image loading for egui_extras (avatars, etc.)
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::install(&cc.egui_ctx, theme::Theme::dark());
            Ok(Box::new(app::AppState::new(config, config_path)))
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    const COOKIE: &str = "_|WARNING:-DO-NOT-SHARE-THIS!--.ABCDEF0123456789";

    /// A secret split across two `write` calls must still be caught. This is
    /// the reason `ScrubbingWriter` buffers instead of scrubbing each chunk.
    #[test]
    fn scrubs_a_secret_that_straddles_two_writes() {
        let sink = Arc::new(Mutex::new(Vec::<u8>::new()));
        {
            let mut w = ScrubbingWriter::new(SharedSink(Arc::clone(&sink)));
            w.write_all(b"cookie=_|WARNING:-DO-NOT-").unwrap();
            w.write_all(b"SHARE-THIS!--.ABCDEF0123456789\n").unwrap();
        }
        let out = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
        assert!(!out.contains("ABCDEF"), "{out}");
        assert!(out.contains("<redacted>"), "{out}");
    }

    /// The whole pipeline as `main` wires it: a real `fmt` subscriber writing
    /// through `Scrubbed`. Guards against the layer being bypassed by the
    /// formatter's own buffering.
    #[test]
    fn a_formatted_event_reaches_the_writer_already_scrubbed() {
        let sink = Arc::new(Mutex::new(Vec::<u8>::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(Scrubbed(SharedSinkMaker(Arc::clone(&sink))))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::error!("leaking {COOKIE} from C:\\Users\\Keeper\\rm.log");
        });

        let out = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
        assert!(!out.is_empty(), "nothing reached the writer");
        assert!(!out.contains("ABCDEF"), "cookie survived: {out}");
        assert!(!out.contains("Keeper"), "username survived: {out}");
        // The non-secret part of the message must still be there, or the log is
        // useless and nobody will keep the layer.
        assert!(out.contains("leaking"), "{out}");
    }

    /// The appender writes a dated file into the data directory, and the
    /// scrubbing layer is in front of it there too (not only in front of the
    /// in-memory sink the tests above use).
    #[test]
    fn the_rotating_appender_writes_a_dated_scrubbed_file() {
        let dir = std::env::temp_dir().join(format!("ram_logtest_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let appender = log_appender(&dir).expect("appender should open in a temp dir");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(Scrubbed(appender))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!("session cookie {COOKIE}");
        });

        let files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        assert_eq!(files.len(), 1, "expected exactly one log file: {files:?}");
        let name = files[0].file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with("rm.") && name.ends_with(".log") && name.len() > "rm..log".len(),
            "not a dated log file: {name}"
        );

        let contents = std::fs::read_to_string(&files[0]).unwrap();
        assert!(contents.contains("session cookie"), "{contents}");
        assert!(
            !contents.contains("ABCDEF"),
            "cookie hit the file: {contents}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // Legacy log purge
    // -----------------------------------------------------------------------

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ram_purge_{tag}_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The pre-rotation file goes, and the rotated ones beside it do not. The
    /// second half is the part worth pinning: those are live history and are
    /// named only one character apart.
    #[test]
    fn the_pre_rotation_log_is_removed_and_the_rotated_ones_are_not() {
        let dir = temp_dir("basic");
        std::fs::write(dir.join("rm.log"), b"gameinfo:LEAKED").unwrap();
        std::fs::write(dir.join("rm.2026-08-05.log"), b"keep me").unwrap();
        std::fs::write(dir.join("rm.2026-08-06.log"), b"keep me too").unwrap();

        purge_legacy_log(&dir);

        assert!(!dir.join("rm.log").exists(), "the legacy log survived");
        assert!(dir.join("rm.2026-08-05.log").is_file());
        assert!(dir.join("rm.2026-08-06.log").is_file());
        assert!(dir.join(LEGACY_LOG_PURGED_MARKER).is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Once, not on every start. A user who deliberately puts an `rm.log` back
    /// (pinning an old session while debugging) must not have RM eat it on the
    /// next launch.
    #[test]
    fn a_recreated_log_is_left_alone_on_later_starts() {
        let dir = temp_dir("once");
        std::fs::write(dir.join("rm.log"), b"old").unwrap();
        purge_legacy_log(&dir);
        assert!(!dir.join("rm.log").exists());

        std::fs::write(dir.join("rm.log"), b"deliberately put back").unwrap();
        purge_legacy_log(&dir);

        assert_eq!(
            std::fs::read_to_string(dir.join("rm.log")).unwrap(),
            "deliberately put back"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A fresh install has nothing to purge and must still stop checking.
    #[test]
    fn a_clean_data_dir_is_marked_without_touching_anything() {
        let dir = temp_dir("clean");
        purge_legacy_log(&dir);
        assert!(dir.join(LEGACY_LOG_PURGED_MARKER).is_file());
        assert!(!dir.join("rm.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `Vec<u8>` behind a shared handle, as a plain `io::Write`.
    struct SharedSink(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedSink {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The same sink as a `MakeWriter`, so it can stand in for the log file.
    struct SharedSinkMaker(Arc<Mutex<Vec<u8>>>);
    impl<'a> MakeWriter<'a> for SharedSinkMaker {
        type Writer = SharedSink;
        fn make_writer(&'a self) -> Self::Writer {
            SharedSink(Arc::clone(&self.0))
        }
    }
}
