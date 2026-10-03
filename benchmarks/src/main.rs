#![cfg_attr(not(windows), allow(unused))]

#[cfg(windows)]
mod job;
mod measurement;
mod stats;

#[cfg(windows)]
mod win;

#[cfg(not(windows))]
compile_error!("rm-performance-benchmark currently supports Windows only.");

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    // Windows executables normally start with a relatively small main-thread stack.
    // Run the benchmark engine on a deliberately sized worker stack so large
    // Windows/system-enumeration frames cannot overflow before sampling begins.
    let worker = std::thread::Builder::new()
        .name("rm-benchmark".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(app::run)?;

    match worker.join() {
        Ok(result) => result,
        Err(_) => anyhow::bail!("benchmark worker thread panicked"),
    }
}

#[cfg(windows)]
mod app {
    use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
    use std::fs::{self, File};
    use std::io::{BufWriter, Read};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::job::Job;
    use crate::measurement::{Counters as CounterTotals, IdlePoint, cpu_pct, is_idle, next_slot};
    use anyhow::{Context, Result, bail};
    use chrono::{Local, Utc};
    use clap::Parser;
    use csv::Writer;
    use serde::Serialize;
    use sha2::{Digest, Sha256};
    use sysinfo::System;

    use crate::stats::{describe, lag1_autocorr, percentile, regression};
    use crate::win;

    const MIB: f64 = 1024.0 * 1024.0;

    #[derive(Parser, Debug)]
    #[command(name = "rm-performance-benchmark")]
    #[command(about = "Native Windows performance benchmark harness for Roblox Manager")]
    struct Args {
        /// Two short runs (10 seconds each); explicit measurement options still win.
        #[arg(long)]
        quick: bool,

        /// Exclude runs unless an instrumented RM sends its React readiness signal.
        #[arg(long)]
        require_ready: bool,

        /// Path to rm_tauri.exe. If omitted, common repository build paths are checked.
        #[arg(long)]
        exe: Option<PathBuf>,

        #[arg(long, default_value_t = 10)]
        runs: usize,

        /// Measured seconds after the app first becomes responsive.
        #[arg(long, default_value_t = 60.0)]
        duration: f64,

        #[arg(long, default_value_t = 100)]
        sample_ms: u64,

        #[arg(long, default_value_t = 2.0)]
        baseline_seconds: f64,

        #[arg(long, default_value_t = 5.0)]
        warmup_seconds: f64,

        #[arg(long, default_value_t = 2.0)]
        inter_run_seconds: f64,

        #[arg(long, default_value_t = 30.0)]
        startup_timeout_seconds: f64,

        #[arg(long, default_value_t = 2.0)]
        idle_window_seconds: f64,

        /// Idle threshold expressed as % of one logical CPU.
        #[arg(long, default_value_t = 5.0)]
        idle_cpu_pct: f64,

        #[arg(long, default_value_t = 1.0)]
        idle_memory_range_mib: f64,

        /// Maximum read + write + other I/O bytes per second for idle detection.
        #[arg(long, default_value_t = 16384.0)]
        idle_io_bytes_per_second: f64,

        #[arg(long)]
        output_dir: Option<PathBuf>,
    }

    #[derive(Debug, Clone, Serialize)]
    struct CsvRow {
        record_type: String,
        session_id: String,
        run: String,
        sample: String,
        timestamp_local: String,
        timestamp_utc: String,
        elapsed_ms: String,
        phase: String,
        metric: String,
        value: String,
        unit: String,
        root_pid: String,
        process_count: String,
        cpu_core_pct: String,
        cpu_machine_pct: String,
        user_cpu_ms: String,
        kernel_cpu_ms: String,
        working_set_bytes: String,
        private_bytes: String,
        virtual_bytes: String,
        peak_working_set_bytes: String,
        handles: String,
        threads: String,
        page_faults: String,
        read_ops_delta: String,
        write_ops_delta: String,
        other_ops_delta: String,
        read_bytes_delta: String,
        write_bytes_delta: String,
        other_bytes_delta: String,
        visible_window: String,
        responsive_window: String,
        system_cpu_pct: String,
        system_available_bytes: String,
        system_memory_pct: String,
        harness_cpu_core_pct: String,
        harness_working_set_bytes: String,
        sampler_lateness_ms: String,
        sampler_work_ms: String,
        webview_process_count: String,
        missing_process_metrics: String,
    }

    impl CsvRow {
        fn blank(kind: &str, session: &str) -> Self {
            Self {
                record_type: kind.into(),
                session_id: session.into(),
                run: String::new(),
                sample: String::new(),
                timestamp_local: String::new(),
                timestamp_utc: String::new(),
                elapsed_ms: String::new(),
                phase: String::new(),
                metric: String::new(),
                value: String::new(),
                unit: String::new(),
                root_pid: String::new(),
                process_count: String::new(),
                cpu_core_pct: String::new(),
                cpu_machine_pct: String::new(),
                user_cpu_ms: String::new(),
                kernel_cpu_ms: String::new(),
                working_set_bytes: String::new(),
                private_bytes: String::new(),
                virtual_bytes: String::new(),
                peak_working_set_bytes: String::new(),
                handles: String::new(),
                threads: String::new(),
                page_faults: String::new(),
                read_ops_delta: String::new(),
                write_ops_delta: String::new(),
                other_ops_delta: String::new(),
                read_bytes_delta: String::new(),
                write_bytes_delta: String::new(),
                other_bytes_delta: String::new(),
                visible_window: String::new(),
                responsive_window: String::new(),
                system_cpu_pct: String::new(),
                system_available_bytes: String::new(),
                system_memory_pct: String::new(),
                harness_cpu_core_pct: String::new(),
                harness_working_set_bytes: String::new(),
                sampler_lateness_ms: String::new(),
                sampler_work_ms: String::new(),
                webview_process_count: String::new(),
                missing_process_metrics: String::new(),
            }
        }

        fn metric(
            kind: &str,
            session: &str,
            run: Option<usize>,
            name: &str,
            value: impl ToString,
            unit: &str,
        ) -> Self {
            let mut row = Self::blank(kind, session);
            row.run = run.map(|r| r.to_string()).unwrap_or_default();
            row.metric = name.into();
            row.value = value.to_string();
            row.unit = unit.into();
            row
        }
    }

    #[derive(Debug, Clone, Default)]
    struct Sample {
        elapsed_ms: f64,
        phase: String,
        process_count: f64,
        cpu_core_pct: f64,
        cpu_machine_pct: f64,
        user_cpu_ms: f64,
        kernel_cpu_ms: f64,
        working_set_bytes: f64,
        private_bytes: f64,
        virtual_bytes: f64,
        peak_working_set_bytes: f64,
        handles: f64,
        threads: f64,
        page_faults: f64,
        read_ops_delta: f64,
        write_ops_delta: f64,
        other_ops_delta: f64,
        read_bytes_delta: f64,
        write_bytes_delta: f64,
        other_bytes_delta: f64,
        system_cpu_pct: f64,
        system_memory_pct: f64,
        harness_cpu_core_pct: f64,
        harness_working_set_bytes: f64,
        sampler_lateness_ms: f64,
        sampler_work_ms: f64,
        missing_process_metrics: f64,
    }

    pub fn run() -> Result<()> {
        eprintln!("[benchmark] startup");
        use clap::{CommandFactory, FromArgMatches, parser::ValueSource};
        let matches = Args::command().get_matches();
        let mut args = Args::from_arg_matches(&matches)?;
        if args.quick {
            if matches.value_source("runs") != Some(ValueSource::CommandLine) {
                args.runs = 2;
            }
            if matches.value_source("duration") != Some(ValueSource::CommandLine) {
                args.duration = 10.0;
            }
            if matches.value_source("warmup_seconds") != Some(ValueSource::CommandLine) {
                args.warmup_seconds = 2.0;
            }
        }
        for (name, value, positive) in [
            ("duration", args.duration, true),
            ("baseline-seconds", args.baseline_seconds, false),
            ("warmup-seconds", args.warmup_seconds, false),
            ("inter-run-seconds", args.inter_run_seconds, false),
            (
                "startup-timeout-seconds",
                args.startup_timeout_seconds,
                true,
            ),
            ("idle-window-seconds", args.idle_window_seconds, true),
            ("idle-cpu-pct", args.idle_cpu_pct, false),
            ("idle-memory-range-mib", args.idle_memory_range_mib, false),
            (
                "idle-io-bytes-per-second",
                args.idle_io_bytes_per_second,
                false,
            ),
        ] {
            if !value.is_finite()
                || value < 0.0
                || (positive && value == 0.0)
                || value > 86400.0 * 365.0
            {
                bail!("invalid --{name}");
            }
        }
        if args.duration <= args.warmup_seconds {
            bail!("--duration must exceed --warmup-seconds");
        }
        eprintln!("[benchmark] arguments parsed");
        if args.runs == 0 {
            bail!("--runs must be >= 1");
        }
        if !(20..=60_000).contains(&args.sample_ms) {
            bail!("--sample-ms must be between 20 and 60000 ms");
        }

        let exe = resolve_exe(args.exe.as_deref())?;
        let output_dir = args
            .output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results"));
        fs::create_dir_all(&output_dir)?;

        let now = Local::now();
        let session = now.format("%Y%m%dT%H%M%S%f%z").to_string();
        let filename = format!("{}_rm-performance.csv", now.format("%Y-%m-%d_%H-%M-%S_%f"));
        let output = output_dir.join(filename);

        println!("Target: {}", exe.display());
        println!("Runs: {}", args.runs);
        println!("Sampling: every {} ms", args.sample_ms);
        println!("Output: {}", output.display());

        let file = File::options().write(true).create_new(true).open(&output)?;
        let mut writer = Writer::from_writer(BufWriter::new(file));

        write_metadata(&mut writer, &session, &exe, &args)?;

        let mut run_summaries: Vec<BTreeMap<String, (f64, &'static str)>> = Vec::new();

        for run_index in 1..=args.runs {
            println!("[{run_index}/{}] benchmarking...", args.runs);
            let summary = match run_once(&mut writer, &session, &exe, run_index, &args) {
                Ok(summary) => summary,
                Err(error) => {
                    eprintln!("Run {run_index} failed: {error:#}");
                    writer.serialize(CsvRow::metric(
                        "diagnostic",
                        &session,
                        Some(run_index),
                        "run.failure",
                        format!("{error:#}"),
                        "",
                    ))?;
                    BTreeMap::from([("run.valid".into(), (0.0, "bool"))])
                }
            };
            for (metric, (value, unit)) in &summary {
                writer.serialize(CsvRow::metric(
                    "run_summary",
                    &session,
                    Some(run_index),
                    metric,
                    fmt(*value),
                    unit,
                ))?;
            }
            writer.flush()?;
            if summary.get("run.valid").is_some_and(|v| v.0 == 1.0) {
                run_summaries.push(summary);
            }

            if run_index < args.runs {
                thread::sleep(Duration::from_secs_f64(args.inter_run_seconds.max(0.0)));
            }
        }

        writer.serialize(CsvRow::metric(
            "aggregate",
            &session,
            None,
            "runs.valid",
            run_summaries.len(),
            "runs",
        ))?;
        writer.serialize(CsvRow::metric(
            "aggregate",
            &session,
            None,
            "runs.excluded",
            args.runs - run_summaries.len(),
            "runs",
        ))?;
        let aggregate = aggregate(&run_summaries);
        for (metric, (value, unit)) in aggregate {
            writer.serialize(CsvRow::metric(
                "aggregate",
                &session,
                None,
                &metric,
                fmt(value),
                unit,
            ))?;
        }
        writer.flush()?;

        println!("Done: {}", output.display());
        Ok(())
    }

    fn resolve_exe(explicit: Option<&Path>) -> Result<PathBuf> {
        if let Some(path) = explicit {
            return path
                .canonicalize()
                .with_context(|| format!("executable not found: {}", path.display()));
        }

        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo = manifest.parent().unwrap_or(&manifest);
        let candidates = [
            repo.join("target/release/rm_tauri.exe"),
            repo.join("target/debug/rm_tauri.exe"),
            repo.join("ram_ui/src-tauri/target/release/rm_tauri.exe"),
            repo.join("ram_ui/src-tauri/target/debug/rm_tauri.exe"),
        ];

        for path in candidates {
            if path.is_file() {
                return path
                    .canonicalize()
                    .context("failed to canonicalize executable");
            }
        }
        bail!("could not auto-detect rm_tauri.exe; pass --exe <path>")
    }

    fn write_metadata<W: std::io::Write>(
        writer: &mut Writer<W>,
        session: &str,
        exe: &Path,
        args: &Args,
    ) -> Result<()> {
        let mut sys = System::new();
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let meta = [
            ("benchmark.schema_version", "3".into(), ""),
            (
                "benchmark.accounting",
                "windows_job_lifetime_cpu_io".into(),
                "",
            ),
            (
                "benchmark.app_ready",
                "react_initial_local_data_then_two_frames".into(),
                "",
            ),
            (
                "benchmark.idle_io_bytes_per_second",
                args.idle_io_bytes_per_second.to_string(),
                "bytes/s",
            ),
            ("benchmark.session_id", session.into(), ""),
            ("benchmark.runs", args.runs.to_string(), "runs"),
            ("benchmark.quick", args.quick.to_string(), "bool"),
            (
                "benchmark.require_ready",
                args.require_ready.to_string(),
                "bool",
            ),
            (
                "benchmark.startup_timeout_seconds",
                args.startup_timeout_seconds.to_string(),
                "s",
            ),
            ("benchmark.sample_ms", args.sample_ms.to_string(), "ms"),
            (
                "benchmark.duration_seconds",
                args.duration.to_string(),
                "s_after_responsive",
            ),
            (
                "benchmark.baseline_seconds",
                args.baseline_seconds.to_string(),
                "s",
            ),
            (
                "benchmark.warmup_seconds",
                args.warmup_seconds.to_string(),
                "s",
            ),
            (
                "benchmark.inter_run_seconds",
                args.inter_run_seconds.to_string(),
                "s",
            ),
            (
                "benchmark.idle_window_seconds",
                args.idle_window_seconds.to_string(),
                "s",
            ),
            (
                "benchmark.idle_cpu_pct",
                args.idle_cpu_pct.to_string(),
                "%_one_logical_cpu",
            ),
            (
                "benchmark.idle_memory_range_mib",
                args.idle_memory_range_mib.to_string(),
                "MiB",
            ),
            ("app.executable", exe.display().to_string(), ""),
            (
                "app.executable_size_bytes",
                fs::metadata(exe)?.len().to_string(),
                "bytes",
            ),
            ("app.executable_sha256", sha256(exe)?, ""),
            (
                "system.os",
                System::long_os_version().unwrap_or_else(|| "unknown".into()),
                "",
            ),
            (
                "system.kernel",
                System::kernel_version().unwrap_or_else(|| "unknown".into()),
                "",
            ),
            (
                "system.host",
                System::host_name().unwrap_or_else(|| "unknown".into()),
                "",
            ),
            ("system.logical_cpus", sys.cpus().len().to_string(), "count"),
            (
                "system.physical_cpus",
                System::physical_core_count()
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                "count",
            ),
            (
                "system.ram_total_bytes",
                sys.total_memory().to_string(),
                "bytes",
            ),
        ];

        for (name, value, unit) in meta {
            writer.serialize(CsvRow::metric("metadata", session, None, name, value, unit))?;
        }

        if let Some(root) = git_output(["rev-parse", "--show-toplevel"]) {
            writer.serialize(CsvRow::metric(
                "metadata", session, None, "git.root", root, "",
            ))?;
        }
        if let Some(commit) = git_output(["rev-parse", "HEAD"]) {
            writer.serialize(CsvRow::metric(
                "metadata",
                session,
                None,
                "git.commit",
                commit,
                "",
            ))?;
        }
        if let Some(branch) = git_output(["branch", "--show-current"]) {
            writer.serialize(CsvRow::metric(
                "metadata",
                session,
                None,
                "git.branch",
                branch,
                "",
            ))?;
        }
        Ok(())
    }

    fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
        Command::new("git").args(args).output().ok().and_then(|o| {
            o.status
                .success()
                .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
    }

    fn sha256(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 1024 * 1024];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn run_once<W: std::io::Write>(
        writer: &mut Writer<W>,
        session: &str,
        exe: &Path,
        run_index: usize,
        args: &Args,
    ) -> Result<BTreeMap<String, (f64, &'static str)>> {
        let interval = Duration::from_millis(args.sample_ms);
        // machine capacity includes every active Windows processor group.
        let logical_cpus =
            unsafe { windows_sys::Win32::System::Threading::GetActiveProcessorCount(0xffff) }
                as usize;

        let mut system = System::new();
        system.refresh_cpu_usage();

        // System baseline before RM launches.
        let baseline_start = Instant::now();
        let mut baseline_sample = 0usize;
        let mut baseline_slot = Duration::ZERO;
        while baseline_start.elapsed().as_secs_f64() < args.baseline_seconds {
            sleep_until(baseline_start + baseline_slot);
            system.refresh_cpu_usage();
            system.refresh_memory();

            let mut row = CsvRow::blank("sample", session);
            let local = Local::now();
            row.run = run_index.to_string();
            row.sample = baseline_sample.to_string();
            row.timestamp_local = local.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            row.timestamp_utc = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            row.elapsed_ms = fmt(baseline_start.elapsed().as_secs_f64() * 1000.0);
            row.phase = "baseline".into();
            row.system_cpu_pct = fmt(system.global_cpu_usage() as f64);
            row.system_available_bytes = system.available_memory().to_string();
            row.system_memory_pct = fmt(memory_pct(&system));
            writer.serialize(row)?;

            baseline_sample += 1;
            baseline_slot = next_slot(baseline_slot, baseline_start.elapsed(), interval);
        }

        let harness_baseline =
            win::native_process(std::process::id(), 0).context("cannot measure harness")?;
        let harness_start = Instant::now();
        let ready = win::Readiness::new(run_index)?;
        let launch = Instant::now();
        let child = Job::launch(exe, &ready.name)?;
        let process_launch_ms = launch.elapsed().as_secs_f64() * 1000.0;
        let root_pid = child.pid;
        let mut samples = Vec::<Sample>::new();
        let mut sample_index = 0usize;
        let mut app_ready_ms = None;
        let mut first_created_ms = None;
        let mut first_visible_ms = None;
        let mut first_responsive_ms = None;
        let mut first_idle_ms = None;
        let mut responsive_at: Option<f64> = None;
        let mut previous = CounterTotals::default();
        let mut native_cpu = win::CpuGroup::default();
        let mut unobserved_processes = 0usize;
        let mut idle_window: VecDeque<IdlePoint> = VecDeque::new();
        let mut previous_at = launch;
        let mut harness_previous =
            harness_baseline.user_cpu_100ns + harness_baseline.kernel_cpu_100ns;
        let mut harness_at = harness_start;
        let mut saw_webview = false;
        let mut flags = HashSet::<&str>::new();
        let mut last_windows = Vec::new();

        let mut slot = next_slot(Duration::ZERO, launch.elapsed(), interval);
        loop {
            sleep_until(launch + slot);
            let work_start = Instant::now();
            let lateness_ms = work_start
                .saturating_duration_since(launch + slot)
                .as_secs_f64()
                * 1000.0;

            if let Some(code) = child.exit_code()? {
                flags.insert("unexpected_root_exit");
                writer.serialize(CsvRow::metric(
                    "diagnostic",
                    session,
                    Some(run_index),
                    "root.exit_code",
                    code,
                    "code",
                ))?;
                break;
            }

            system.refresh_cpu_usage();
            system.refresh_memory();
            let pids = child.pids()?;
            let snapshot = win::process_snapshot()?;
            let webviews = pids
                .iter()
                .filter(|p| snapshot.get(p).is_some_and(|v| v.webview))
                .count();
            saw_webview |= webviews > 0;
            if pids.is_empty() {
                flags.insert("unexpected_empty_job");
                break;
            }

            let window_observed_at = Instant::now();
            let elapsed_ms = window_observed_at.duration_since(launch).as_secs_f64() * 1000.0;
            let elapsed_s = elapsed_ms / 1000.0;
            if app_ready_ms.is_none() && ready.signaled() {
                app_ready_ms = Some(elapsed_ms);
            }

            let (created, visible, responsive, windows) =
                win::visible_window_state(&HashSet::from([root_pid]));
            if created && first_created_ms.is_none() {
                first_created_ms = Some(elapsed_ms);
            }
            last_windows = windows;

            if visible && first_visible_ms.is_none() {
                first_visible_ms = Some(elapsed_ms);
            }
            if responsive && first_responsive_ms.is_none() {
                first_responsive_ms = Some(elapsed_ms);
                responsive_at = Some(elapsed_s);
            }

            let phase = match responsive_at {
                None => "startup",
                Some(t) if elapsed_s < t + args.warmup_seconds => "warmup",
                Some(_) => "steady",
            };

            let (native_user, native_kernel, observed) = native_cpu.totals(&pids, &child)?;
            let (mut counters, faults, total_processes) = child.counters()?;
            unobserved_processes = (total_processes as usize).saturating_sub(observed);
            if unobserved_processes == 0 {
                counters.user_100ns = native_user;
                counters.kernel_100ns = native_kernel;
            } else {
                // job lifetime totals recover children that lived entirely between samples.
                counters.user_100ns = counters.user_100ns.max(native_user);
                counters.kernel_100ns = counters.kernel_100ns.max(native_kernel);
            }
            let counter_at = Instant::now();
            let sample_elapsed_ms = counter_at.duration_since(launch).as_secs_f64() * 1000.0;
            let mut s = collect_tree(
                &child,
                &snapshot,
                &pids,
                logical_cpus,
                previous,
                counters,
                counter_at.duration_since(previous_at),
            );
            s.page_faults = faults as f64;
            previous = counters;
            previous_at = counter_at;
            s.sampler_lateness_ms = lateness_ms;
            if s.missing_process_metrics > 0.0 {
                flags.insert("incomplete_process_metrics");
            }
            if lateness_ms > args.sample_ms as f64 {
                flags.insert("excessive_sampler_lateness");
            }

            let local = Local::now();
            let mut row = CsvRow::blank("sample", session);
            row.run = run_index.to_string();
            row.sample = sample_index.to_string();
            row.timestamp_local = local.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            row.timestamp_utc = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            row.elapsed_ms = fmt(sample_elapsed_ms);
            row.phase = phase.into();
            row.root_pid = root_pid.to_string();
            row.process_count = fmt(s.process_count);
            row.cpu_core_pct = fmt(s.cpu_core_pct);
            row.cpu_machine_pct = fmt(s.cpu_machine_pct);
            row.user_cpu_ms = fmt(s.user_cpu_ms);
            row.kernel_cpu_ms = fmt(s.kernel_cpu_ms);
            row.working_set_bytes = fmt(s.working_set_bytes);
            row.private_bytes = fmt(s.private_bytes);
            row.virtual_bytes = fmt(s.virtual_bytes);
            row.peak_working_set_bytes = fmt(s.peak_working_set_bytes);
            row.handles = fmt(s.handles);
            row.threads = fmt(s.threads);
            row.page_faults = fmt(s.page_faults);
            row.read_ops_delta = fmt(s.read_ops_delta);
            row.write_ops_delta = fmt(s.write_ops_delta);
            row.other_ops_delta = fmt(s.other_ops_delta);
            row.read_bytes_delta = fmt(s.read_bytes_delta);
            row.write_bytes_delta = fmt(s.write_bytes_delta);
            row.other_bytes_delta = fmt(s.other_bytes_delta);
            row.visible_window = if visible { "1" } else { "0" }.into();
            row.responsive_window = if responsive { "1" } else { "0" }.into();
            row.system_cpu_pct = fmt(system.global_cpu_usage() as f64);
            row.system_available_bytes = system.available_memory().to_string();
            row.system_memory_pct = fmt(memory_pct(&system));

            let me = win::native_process(std::process::id(), 0)
                .context("harness metrics unavailable")?;
            let now = Instant::now();
            let ticks = me.user_cpu_100ns + me.kernel_cpu_100ns;
            row.harness_cpu_core_pct = fmt(cpu_pct(
                ticks.saturating_sub(harness_previous),
                now.duration_since(harness_at),
                1,
            )
            .0);
            row.harness_working_set_bytes = me.working_set_bytes.to_string();
            harness_previous = ticks;
            harness_at = now;
            s.sampler_work_ms = work_start.elapsed().as_secs_f64() * 1000.0;
            row.sampler_lateness_ms = fmt(lateness_ms);
            row.sampler_work_ms = fmt(s.sampler_work_ms);
            row.webview_process_count = webviews.to_string();
            row.missing_process_metrics = fmt(s.missing_process_metrics);
            writer.serialize(&row)?;
            let mut stored = s.clone();
            stored.elapsed_ms = sample_elapsed_ms;
            stored.phase = phase.into();
            stored.system_cpu_pct = system.global_cpu_usage() as f64;
            stored.system_memory_pct = memory_pct(&system);
            stored.harness_cpu_core_pct = row.harness_cpu_core_pct.parse().unwrap_or(0.0);
            stored.harness_working_set_bytes = row.harness_working_set_bytes.parse().unwrap_or(0.0);
            samples.push(stored);

            if phase == "steady" && first_idle_ms.is_none() {
                idle_window.push_back(IdlePoint {
                    seconds: elapsed_s,
                    cpu: s.cpu_core_pct,
                    working_mib: s.working_set_bytes / MIB,
                    private_mib: s.private_bytes / MIB,
                    io_bytes: s.read_bytes_delta + s.write_bytes_delta + s.other_bytes_delta,
                });
                while idle_window.len() > 2
                    && elapsed_s - idle_window[1].seconds >= args.idle_window_seconds
                {
                    idle_window.pop_front();
                }
                if is_idle(
                    idle_window.make_contiguous(),
                    args.idle_window_seconds,
                    args.idle_cpu_pct,
                    args.idle_memory_range_mib,
                    args.idle_io_bytes_per_second,
                ) {
                    first_idle_ms = Some(elapsed_ms);
                }
            }
            slot = next_slot(slot, launch.elapsed(), interval);
            sample_index += 1;

            if let Some(t) = responsive_at {
                if elapsed_s >= t + args.duration {
                    break;
                }
            } else if elapsed_s >= args.startup_timeout_seconds {
                break;
            }
        }

        if app_ready_ms.is_none() {
            writer.serialize(CsvRow::metric(
                "diagnostic",
                session,
                Some(run_index),
                "app_ready_unavailable",
                1,
                "bool",
            ))?;
            if args.require_ready {
                flags.insert("missing_app_ready");
            }
        }
        if !saw_webview {
            flags.insert("missing_webview2");
        }
        if first_responsive_ms.is_none() {
            flags.insert("startup_timeout");
        }
        if !close_process_tree(&child, &last_windows)? {
            flags.insert("forced_cleanup");
        }
        let mut summary = summarize_run(
            &samples,
            first_visible_ms,
            first_responsive_ms,
            first_idle_ms,
        );
        insert_opt(&mut summary, "startup.app_ready_ms", app_ready_ms, "ms");
        insert_opt(
            &mut summary,
            "startup.first_created_ms",
            first_created_ms,
            "ms",
        );
        summary.insert(
            "cpu.unobserved_processes".into(),
            (unobserved_processes as f64, "count"),
        );
        summary.insert(
            "startup.process_launch_ms".into(),
            (process_launch_ms, "ms"),
        );
        summary.insert(
            "run.valid".into(),
            (if flags.is_empty() { 1.0 } else { 0.0 }, "bool"),
        );
        let mut flags: Vec<_> = flags.into_iter().collect();
        flags.sort();
        for flag in flags {
            eprintln!("Run {run_index}: {flag}");
            writer.serialize(CsvRow::metric(
                "diagnostic",
                session,
                Some(run_index),
                flag,
                1,
                "bool",
            ))?;
        }
        Ok(summary)
    }

    fn sleep_until(deadline: Instant) {
        let now = Instant::now();
        if deadline > now {
            thread::sleep(deadline - now);
        }
    }

    fn close_process_tree(
        child: &Job,
        windows: &[windows_sys::Win32::Foundation::HWND],
    ) -> Result<bool> {
        win::request_close(windows);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if child.pids()?.is_empty() && child.exit_code()?.is_some() {
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(50));
        }
        child.terminate()?;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if child.pids()?.is_empty() && child.exit_code()?.is_some() {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(50));
        }
        bail!("cleanup failed: run job still has active processes")
    }

    fn memory_pct(system: &System) -> f64 {
        if system.total_memory() == 0 {
            0.0
        } else {
            system.used_memory() as f64 / system.total_memory() as f64 * 100.0
        }
    }

    fn collect_tree(
        job: &Job,
        snapshot: &HashMap<u32, win::ProcessEntry>,
        pids: &HashSet<u32>,
        logical_cpus: usize,
        previous: CounterTotals,
        counters: CounterTotals,
        elapsed: Duration,
    ) -> Sample {
        let mut sample = Sample {
            process_count: pids.len() as f64,
            ..Default::default()
        };

        for &pid in pids {
            let entry = snapshot.get(&pid);
            let threads = entry.map(|p| p.threads).unwrap_or(0);
            if let Some(native) = win::native_process_for_job(pid, threads, Some(job)) {
                if entry.is_none() {
                    sample.missing_process_metrics += 1.0;
                }
                sample.virtual_bytes += native.virtual_bytes as f64;
                sample.working_set_bytes += native.working_set_bytes as f64;
                sample.private_bytes += native.private_bytes as f64;
                sample.peak_working_set_bytes += native.peak_working_set_bytes as f64;
                sample.handles += native.handles as f64;
                sample.threads += native.threads as f64;
                sample.page_faults += native.page_faults as f64;
            } else {
                sample.missing_process_metrics += 1.0;
            }
        }
        let delta = counters.delta(previous);
        sample.cpu_core_pct =
            cpu_pct(delta.user_100ns + delta.kernel_100ns, elapsed, logical_cpus).0;
        sample.cpu_machine_pct = sample.cpu_core_pct / logical_cpus.max(1) as f64;
        sample.user_cpu_ms = counters.user_100ns as f64 / 10_000.0;
        sample.kernel_cpu_ms = counters.kernel_100ns as f64 / 10_000.0;
        sample.read_ops_delta = counters.read_ops.saturating_sub(previous.read_ops) as f64;
        sample.write_ops_delta = counters.write_ops.saturating_sub(previous.write_ops) as f64;
        sample.other_ops_delta = counters.other_ops.saturating_sub(previous.other_ops) as f64;
        sample.read_bytes_delta = counters.read_bytes.saturating_sub(previous.read_bytes) as f64;
        sample.write_bytes_delta = counters.write_bytes.saturating_sub(previous.write_bytes) as f64;
        sample.other_bytes_delta = counters.other_bytes.saturating_sub(previous.other_bytes) as f64;

        sample
    }

    fn summarize_run(
        samples: &[Sample],
        first_visible_ms: Option<f64>,
        first_responsive_ms: Option<f64>,
        first_idle_ms: Option<f64>,
    ) -> BTreeMap<String, (f64, &'static str)> {
        let mut out = BTreeMap::new();
        insert_opt(&mut out, "startup.first_visible_ms", first_visible_ms, "ms");
        insert_opt(
            &mut out,
            "startup.first_responsive_ms",
            first_responsive_ms,
            "ms",
        );
        insert_opt(&mut out, "idle.first_idle_ms", first_idle_ms, "ms");

        out.insert("samples.total".into(), (samples.len() as f64, "rows"));

        let phases = [
            ("all", samples.iter().collect::<Vec<_>>()),
            (
                "startup",
                samples.iter().filter(|s| s.phase == "startup").collect(),
            ),
            (
                "steady",
                samples.iter().filter(|s| s.phase == "steady").collect(),
            ),
        ];

        for (phase_name, rows) in phases {
            if rows.is_empty() {
                continue;
            }

            macro_rules! metric {
                ($field:ident, $unit:expr) => {{
                    let values: Vec<f64> = rows.iter().map(|s| s.$field).collect();
                    add_stats(
                        &mut out,
                        &format!("{phase_name}.{}", stringify!($field)),
                        &values,
                        $unit,
                    );
                }};
            }

            metric!(cpu_core_pct, "%");
            metric!(cpu_machine_pct, "%");
            metric!(working_set_bytes, "bytes");
            metric!(private_bytes, "bytes");
            metric!(virtual_bytes, "bytes");
            metric!(process_count, "count");
            metric!(handles, "count");
            metric!(threads, "count");
            metric!(system_cpu_pct, "%");
            metric!(system_memory_pct, "%");
            metric!(harness_cpu_core_pct, "%");
            metric!(harness_working_set_bytes, "bytes");
            metric!(sampler_lateness_ms, "ms");
            metric!(sampler_work_ms, "ms");
            metric!(page_faults, "count");
        }

        let steady: Vec<&Sample> = samples.iter().filter(|s| s.phase == "steady").collect();
        if steady.len() >= 2 {
            let xs: Vec<f64> = steady.iter().map(|s| s.elapsed_ms / 60_000.0).collect();

            for (name, ys) in [
                (
                    "working_set",
                    steady
                        .iter()
                        .map(|s| s.working_set_bytes / MIB)
                        .collect::<Vec<_>>(),
                ),
                (
                    "private",
                    steady
                        .iter()
                        .map(|s| s.private_bytes / MIB)
                        .collect::<Vec<_>>(),
                ),
            ] {
                if let Some((slope, r2)) = regression(&xs, &ys) {
                    out.insert(
                        format!("steady.{name}.slope_mib_per_min"),
                        (slope, "MiB/min"),
                    );
                    out.insert(format!("steady.{name}.slope_r2"), (r2, "r2"));
                }
            }

            let ws: Vec<f64> = steady.iter().map(|s| s.working_set_bytes).collect();
            if !ws.is_empty() {
                let med = percentile(&ws, 0.5);
                if med != 0.0 {
                    out.insert(
                        "steady.working_set.peak_to_median_ratio".into(),
                        (ws.iter().copied().fold(0.0, f64::max) / med, "ratio"),
                    );
                    out.insert(
                        "steady.working_set.p95_to_median_ratio".into(),
                        (percentile(&ws, 0.95) / med, "ratio"),
                    );
                }
            }
        }

        let total_read = samples.iter().map(|s| s.read_bytes_delta).sum::<f64>();
        let total_write = samples.iter().map(|s| s.write_bytes_delta).sum::<f64>();
        let total_other = samples.iter().map(|s| s.other_bytes_delta).sum::<f64>();
        out.insert("io.read_bytes.total".into(), (total_read, "bytes"));
        out.insert("io.write_bytes.total".into(), (total_write, "bytes"));
        out.insert("io.other_bytes.total".into(), (total_other, "bytes"));

        if let Some(last) = samples.last() {
            let seconds = (last.elapsed_ms / 1000.0).max(0.001);
            out.insert(
                "io.total_bytes_per_second".into(),
                (
                    (total_read + total_write + total_other) / seconds,
                    "bytes/s",
                ),
            );

            out.insert(
                "cpu.time_weighted_mean_core_pct".into(),
                (
                    (last.user_cpu_ms + last.kernel_cpu_ms) / 1000.0 / seconds * 100.0,
                    "%",
                ),
            );
            out.insert(
                "cpu.total_cpu_seconds".into(),
                ((last.user_cpu_ms + last.kernel_cpu_ms) / 1000.0, "cpu_s"),
            );
        }

        out
    }

    fn add_stats(
        out: &mut BTreeMap<String, (f64, &'static str)>,
        prefix: &str,
        values: &[f64],
        unit: &'static str,
    ) {
        let Some(s) = describe(values) else {
            return;
        };
        let stats = [
            ("n", s.n as f64),
            ("min", s.min),
            ("max", s.max),
            ("mean", s.mean),
            ("median", s.median),
            ("variance", s.variance),
            ("stdev", s.stdev),
            ("sem", s.sem),
            ("mean_ci95_low", s.ci95_low),
            ("mean_ci95_high", s.ci95_high),
            ("cv_pct", s.cv_pct),
            ("mad", s.mad),
            ("iqr", s.iqr),
            ("trimmed_mean_5pct", s.trimmed_mean_5pct),
            ("rms", s.rms),
            ("skewness", s.skewness),
            ("excess_kurtosis", s.excess_kurtosis),
            ("p01", s.p01),
            ("p05", s.p05),
            ("p10", s.p10),
            ("p25", s.p25),
            ("p50", s.p50),
            ("p75", s.p75),
            ("p90", s.p90),
            ("p95", s.p95),
            ("p99", s.p99),
            ("lag1_autocorr", lag1_autocorr(values)),
        ];
        for (name, value) in stats {
            if value.is_finite() {
                out.insert(format!("{prefix}.{name}"), (value, stat_unit(name, unit)));
            }
        }
    }

    fn stat_unit(name: &str, unit: &'static str) -> &'static str {
        match name {
            "n" => "count",
            "cv_pct" => "%",
            "lag1_autocorr" | "skewness" | "excess_kurtosis" => "ratio",
            "variance" => match unit {
                "bytes" => "bytes^2",
                "ms" => "ms^2",
                "%" => "%^2",
                "count" => "count^2",
                _ => "squared_original_unit",
            },
            _ => unit,
        }
    }

    fn insert_opt(
        out: &mut BTreeMap<String, (f64, &'static str)>,
        name: &str,
        value: Option<f64>,
        unit: &'static str,
    ) {
        if let Some(v) = value {
            out.insert(name.into(), (v, unit));
        }
    }

    fn aggregate(
        runs: &[BTreeMap<String, (f64, &'static str)>],
    ) -> BTreeMap<String, (f64, &'static str)> {
        let mut by_metric: BTreeMap<String, (Vec<f64>, &'static str)> = BTreeMap::new();
        for run in runs {
            for (metric, (value, unit)) in run {
                if value.is_finite() {
                    let entry = by_metric
                        .entry(metric.clone())
                        .or_insert_with(|| (Vec::new(), *unit));
                    entry.0.push(*value);
                }
            }
        }

        let mut out = BTreeMap::new();
        for (metric, (values, unit)) in by_metric {
            if let Some(s) = describe(&values) {
                let stats = [
                    ("n", s.n as f64),
                    ("min", s.min),
                    ("max", s.max),
                    ("mean", s.mean),
                    ("median", s.median),
                    ("variance", s.variance),
                    ("stdev", s.stdev),
                    ("sem", s.sem),
                    ("mean_ci95_low", s.ci95_low),
                    ("mean_ci95_high", s.ci95_high),
                    ("cv_pct", s.cv_pct),
                    ("mad", s.mad),
                    ("iqr", s.iqr),
                    ("trimmed_mean_5pct", s.trimmed_mean_5pct),
                    ("p05", s.p05),
                    ("p50", s.p50),
                    ("p95", s.p95),
                    ("p99", s.p99),
                ];
                for (name, value) in stats {
                    if value.is_finite() {
                        out.insert(format!("{metric}.{name}"), (value, stat_unit(name, unit)));
                    }
                }
            }
        }
        out
    }

    fn fmt(value: f64) -> String {
        if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.6}")
        }
    }
}
