// The release binary is a GUI app with no console. Under `cargo test` the same
// attribute would detach the test harness from its console and swallow every
// result, so it is applied only to non-test builds.
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod bridge;
mod browser_login;
mod components;
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

/// Canonical data directory: `%APPDATA%\RM`.
pub fn data_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("RM")
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

    // Log to a file so crashes are visible even without a console
    // (the #[windows_subsystem = "windows"] attribute suppresses stderr).
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        );

    match log_appender(&data_dir) {
        Some(appender) => subscriber.with_writer(Scrubbed(appender)).init(),
        // No writable log directory. stderr goes nowhere under
        // `windows_subsystem = "windows"`, so this run is effectively silent,
        // but the app must still start.
        None => subscriber.init(),
    }

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
        let code = browser_login::run_browse_as_child(profile_dir, cookie_in, label);
        std::process::exit(code);
    }

    // Offer to migrate legacy data from the exe directory
    maybe_migrate_legacy_data(&data_dir);

    // Migrate legacy `favorite_places` (inline in config.json) into the new
    // per-file preset system. Runs once: when the migration succeeds we clear
    // the config field so subsequent startups skip the loop.
    maybe_migrate_favorites(&data_dir);

    // Resolve config and account paths.
    // If a legacy config.json still exists next to the exe (user declined migration),
    // keep using local paths for backwards compatibility.
    let (config_path, config) = if PathBuf::from("config.json").is_file()
        && !data_dir.join("config.json").is_file()
    {
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

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([config.window_width, config.window_height])
            .with_min_inner_size([640.0, 400.0])
            .with_title(format!("RM | Roblox Manager v{}", env!("CARGO_PKG_VERSION")))
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
        assert!(!contents.contains("ABCDEF"), "cookie hit the file: {contents}");

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
