# Run the Roblox Manager performance benchmark

This benchmark is **public and reproducible, and anyone can run it themselves**.
It launches a Windows build of Roblox Manager (RM), measures RM together with its
WebView2 browser, renderer, GPU and utility processes, then closes that run.
It produces **CSV only**: raw samples, per-run statistics and cross-run statistics
in one timestamped file. There are no built-in performance claims or scores.

> [!IMPORTANT]
> **For accurate results:**
> 1. Close unnecessary applications first, including any existing RM windows.
> 2. Start the benchmark.
> 3. **Do not touch or use the computer while the benchmark is running.**
> 4. Wait until it fully finishes before using the machine again.
>
> Background activity and actively using the computer can distort CPU, memory,
> I/O and startup results. Avoid moving the mouse, switching windows, gaming,
> browsing or running builds during measurement. Keep the machine awake, plugged
> in and on the same power plan across comparisons.

## Requirements

- A Windows 10 or 11 **64-bit** PC with Microsoft WebView2 Runtime installed.
- Rust stable with the MSVC toolchain, Visual Studio Build Tools (Desktop
  development with C++) and a Windows SDK to build the harness.
- An RM executable: either a downloaded Windows release or a production build.
  You do not need Node.js or pnpm when measuring a downloaded executable.
- To build RM from source, also install Node.js 22+ and pnpm 11+; follow the root
  [build instructions](../README.md#building-from-source).

No administrator access is normally needed. Run RM normally once to complete
first-run setup, then close it. The benchmark uses your normal RM data and
settings; it does not import credentials, clear caches, reset configuration or
launch Roblox games. A locked password store, walkthrough, update check or a
large account collection can change the workload. Record the setup you compare.
Roblox need not be running for this idle-manager benchmark.

## Build

Open PowerShell in the repository root. If you do not have the source yet:

```powershell
git clone https://github.com/Paryx-games/roblox-manager.git
cd roblox-manager
```

Build the independent benchmark crate:

```powershell
cargo build --release --manifest-path .\benchmarks\Cargo.toml
```

The binary is `benchmarks\target\release\rm-performance-benchmark.exe`. The
benchmark has its own Cargo workspace and lockfile.

For the optional React readiness measurement, build this version of RM:

```powershell
pnpm --dir ram_ui install --frozen-lockfile
pnpm --dir ram_ui tauri build --ci --no-bundle -- --locked
```

This produces `target\release\rm_tauri.exe` with the production frontend embedded.
Avoid a Vite development server or a debug executable for published comparisons.

## Quick mode: check that everything works

```powershell
.\benchmarks\target\release\rm-performance-benchmark.exe --quick
```

Quick mode runs **two launches with 10 seconds after responsiveness per run**,
including a 2-second warmup. It normally finishes in under a minute. Use it to
check the executable, process accounting and CSV, not to establish a performance
baseline. Explicit options override quick defaults, for example
`--quick --runs 1 --duration 5 --warmup-seconds 1`.

If the app is elsewhere, select it explicitly (this works in every mode):

```powershell
.\benchmarks\target\release\rm-performance-benchmark.exe --quick --exe 'C:\path\to\Roblox Manager.exe'
```

Without `--exe`, the harness checks common repository build paths, preferring
`target\release\rm_tauri.exe`. The target and output path are printed before runs.

## Normal benchmark: about 10 minutes

```powershell
.\benchmarks\target\release\rm-performance-benchmark.exe
```

The standard **10-run benchmark takes roughly 10 minutes** (usually a little
longer with startup, baseline and cleanup). Defaults are:

| Setting | Default |
| --- | --- |
| Runs | 10 |
| Sampling | 100 ms on a monotonic deadline grid |
| System baseline before each launch | 2 seconds |
| Measurement after first responsive window | 60 seconds |
| Warmup within that measurement | 5 seconds |
| Pause between runs | 2 seconds |
| Startup timeout | 30 seconds |
| Graceful close / forced-cleanup confirmation | Up to 3 seconds each |

## Longer comparisons

For more repeatability, use 30 runs and a longer steady observation:

```powershell
.\benchmarks\target\release\rm-performance-benchmark.exe --runs 30 --duration 120 --warmup-seconds 10 --require-ready
```

This takes approximately an hour. `--require-ready` excludes runs unless the
instrumented RM build sends the readiness signal; omit it for older downloaded
releases. More repetitions cannot compensate for a busy machine. Compare the
same settings, account dataset, display scaling, window state, WebView2 version,
power plan and Windows environment. Try alternating which app build runs first.

All options are available through:

```powershell
.\benchmarks\target\release\rm-performance-benchmark.exe --help
```

The equivalent Cargo command is
`cargo run --release --manifest-path .\benchmarks\Cargo.toml -- --quick` (replace
`--quick` with your other options). Build first so compilation does not overlap
with the benchmark.

## Find the CSV

Each session writes exactly one new file under `benchmarks\results\`, such as
`2026-10-02_23-57-26_773389200_rm-performance.csv`. Use `--output-dir 'C:\results'`
to select another directory. Wait for the final **Done** message before opening
it in Excel or another application. Files are flushed after each run and at
completion; interrupted sessions can be incomplete.

| `record_type` | Contents |
| --- | --- |
| `metadata` | Schema, configuration, executable size/SHA256, OS, CPU/RAM and available Git revision information |
| `sample` | Raw baseline/startup/warmup/steady readings |
| `diagnostic` | Failures, suspicious conditions and unavailable readiness |
| `run_summary` | Statistics and startup timings for one run, including `run.valid` |
| `aggregate` | Statistics across valid run summaries, plus `runs.valid` and `runs.excluded` |

Samples have measurement columns. Other rows primarily use `metric`, `value`
and `unit`; unused fields are empty. Baseline samples measure the machine before
RM launches and have no app CPU/memory readings. A diagnostic remains in the
same CSV even when a run fails. Invalid runs retain their raw data and available
per-run statistics but are excluded from aggregate performance statistics.

CSV metadata includes local executable paths and machine names. Review those
identifiers before sharing a file. No account names, cookies, tokens, process
command lines or app memory contents are recorded.

## Important metrics

| Column or summary metric | Meaning |
| --- | --- |
| `elapsed_ms` | Monotonic time since launch request (baseline uses its own origin) |
| `cpu_core_pct` | **100% = one logical CPU**; parallel processes can exceed 100% |
| `cpu_machine_pct` | **100% = the whole machine**; core percentage divided by active logical CPUs across Windows processor groups |
| `user_cpu_ms`, `kernel_cpu_ms` | Cumulative group CPU time, including observed exited processes and job accounting for unseen processes |
| `working_set_bytes` | Sum of resident process working sets, including shared pages counted once per process |
| `private_bytes` | Sum of private committed memory; includes memory that may be paged out |
| `virtual_bytes` | Sum of allocated/reserved virtual address space, **not physical RAM**; WebView2 can reserve many terabytes |
| `peak_working_set_bytes` | Sum of current members' lifetime working-set peaks; these peaks need not occur simultaneously |
| `process_count`, `webview_process_count` | Active job members and members named `msedgewebview2.exe` at the sample |
| `threads`, `handles` | Sum of current process thread/handle counts |
| `page_faults` | Cumulative job page faults (soft and hard), including terminated members |
| `*_ops_delta`, `*_bytes_delta` | Read/write/other I/O since the previous counter sample; not necessarily physical disk traffic |
| `harness_cpu_core_pct`, `harness_working_set_bytes` | Benchmark process CPU/memory, separate from RM; not subtracted from app readings |
| `sampler_work_ms` | Time collecting and preparing a sample, excluding CSV serialization |
| `sampler_lateness_ms` | Delay reaching the scheduled sample; missed slots are skipped, not replayed |
| `missing_process_metrics` | Processes whose detailed measurements or thread snapshot were unavailable; treat these samples as incomplete |
| `startup.process_launch_ms` | Launch request through process creation, job assignment and resume |
| `startup.first_created_ms` | First observation of the root's titled RM window, including when hidden |
| `startup.first_visible_ms` | First observation of that window being visible |
| `startup.first_responsive_ms` | First successful bounded `WM_NULL` response from a visible RM window |
| `startup.app_ready_ms` | First observation of the optional React readiness event |
| `idle.first_idle_ms` | End of the first qualifying steady-state idle window; empty if never detected |
| `cpu.unobserved_processes` | Processes counted by the job but never sampled directly; their CPU/I/O are recovered through job lifetime accounting |
| `steady.*.slope_mib_per_min`, `slope_r2` | Linear memory trend and goodness of fit; a short positive slope is not proof of a leak |

### Full RM/WebView2 accounting

The harness creates an unnamed Windows Job Object with kill-on-close enabled.
It starts RM **suspended**, assigns it to the job, then resumes its main thread.
Children inherit membership, including WebView2's browser, renderer, GPU and
utility subprocesses and their nested jobs. Membership comes from the job, not
parent-PID traversal or executable-name matching. Other running RM/browser
processes are never selected for cleanup. Assignment failure cancels the run;
there is no silent fallback to partial process-tree tracking.

CPU uses native **`GetProcessTimes`** user/kernel deltas divided by actual
monotonic elapsed time. Retained process handles and PID/creation-time identities
preserve terminal counters after exit and prevent PID-reuse mistakes. Job
lifetime CPU accounting fills gaps from processes that were born and died
between samples. Job lifetime I/O counters likewise retain exited-child work.
Memory is read through `GetProcessMemoryInfo` and native `ProcessVmCounters`;
thread counts come from a Windows process snapshot. The APIs are sequential,
so samples are near-simultaneous rather than atomic across the whole group.
See Microsoft's [Job Objects documentation](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

After measurement the harness posts `WM_CLOSE`, waits for the job and root to
finish, then terminates only remaining job members if necessary. Forced cleanup
is reported; failure to confirm cleanup invalidates the run. RAII closes owned
handles and kill-on-close contains surviving members on early errors.

### Readiness and idle

Only benchmark launches receive `RM_BENCHMARK=1` and a unique per-run event name
in their child environment. Instrumented RM enables the frontend hook only in
this mode. The main React shell signals after successful initial local
account/instance data loads, settings and startup information, followed by two
animation frames. This is a defined readiness milestone, not completion of all
network requests, avatar loads, tours, password entry or every page's work.
Normal launches do not enable the hook or send benchmark IPC.

Older executables still run: `startup.app_ready_ms` is absent and
`app_ready_unavailable` is recorded. Window responsiveness remains the timing
origin for warmup and the observation duration. Use `--require-ready` when
comparing instrumented builds. Window and event timings are observations with
approximately one sample interval of quantization plus scheduler/collection
latency; they are not exact creation/paint timestamps.

Idle requires a full elapsed window (default 2 seconds) with all sampled CPU
values at or below 5% of one core, working-set **and** private-memory ranges at
or below 1 MiB, and read/write/other I/O at or below 16 KiB/s. Configure these
with `--idle-window-seconds`, `--idle-cpu-pct`, `--idle-memory-range-mib` and
`--idle-io-bytes-per-second`. Idle is a heuristic; periodic background activity
can prevent detection without making a run invalid.

## Statistics and responsible interpretation

Per-run summaries distinguish all/startup/steady samples. They include mean,
median, min/max, sample standard deviation and variance, p95/p99 (linearly
interpolated), CV, standard error, Student-t 95% mean confidence intervals,
MAD/IQR, trimmed mean, RMS and autocorrelation. Memory trends use regression
against actual elapsed minutes. CPU seconds and I/O totals/throughput come from
lifetime counters and measured time rather than assuming every interval is
exactly 100 ms. Aggregate rows describe each summary metric across valid runs.
Counts, ratios and percentage statistics have their own units. No confidence
interval is emitted for a single observation.

- Consecutive samples are correlated; sample-level confidence intervals are
  descriptive, not evidence of hundreds of independent trials. Prefer the
  aggregate interval for **run means**, and inspect run-to-run variation. Even
  runs share OS caches and may not be fully independent.
- Missing WebView2, startup timeouts, early root exits (including crashes),
  incomplete process measurements, excessive lateness and forced/failed cleanup
  are reported. Check `run.valid`, diagnostics and excluded-run counts first.
  No valid runs means there are no aggregate performance conclusions.
- This is idle-manager/startup performance, not a Roblox FPS benchmark or a
  test of game launching, uploads, interactive work or every account operation.
- Caches are not flushed. The benchmark does not claim cold-start timings.
- Working-set sums double-count shared pages; virtual reservations can be huge
  without consuming equivalent RAM. Compare memory definitions consistently.
- Native virtual-memory querying uses an internal Windows information class
  that can change; query failures are reported rather than silently becoming
  zero. CPU counter granularity makes some 100 ms idle samples zero.
- Harness overhead and background contention influence the machine. Inspect
  the harness and timing columns; do not automatically subtract overhead from
  RM CPU or treat a quiet quick run as a guarantee.
- Use the CSV's executable hash to identify the actual build. Git metadata
  identifies the source checkout, which may differ from a downloaded binary or
  contain uncommitted changes. Preserve hardware/settings context alongside
  results, and do not publish unsupported universal performance claims.

## Check the benchmark itself

From the repository root:

```powershell
cargo fmt --manifest-path .\benchmarks\Cargo.toml -- --check
cargo clippy --release --manifest-path .\benchmarks\Cargo.toml -- -D warnings
cargo test --release --manifest-path .\benchmarks\Cargo.toml
cargo build --release --manifest-path .\benchmarks\Cargo.toml
```

Tests cover statistics, CPU units and elapsed-time normalization, lifecycle
accounting, idle criteria and deadline scheduling. Windows integration tests
also exercise real child exit/job cleanup, native counters and readiness events
using synthetic work only.
