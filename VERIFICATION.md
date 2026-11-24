# Rust release verification

## November 24, 2026 built-in Retina benchmark

On a 10-core M2 Pro with 16 GiB RAM running macOS 27.2.1 (26B131), Task Manager 2.1.0 averaged **2.63% of one CPU core**, **93.56 MiB resident memory** and **30.72 MiB physical footprint** across 3 fresh launches. The previous 500 ms release used 107.40 MiB resident in one matched launch, a difference of 13.84 MiB (12.9%).

Both builds displayed the grouped Processes page, sorted by Name, in an 1120 × 720-point window at the default 500 ms refresh. The built-in display was set to 1512 × 982 points at 2× scale. Each launch warmed for 60 seconds before 20 one-second kernel readings. Other apps remained open. All 332 window checks used the same display, including 61 checks per warm-up and 22 around each measurement interval. The window was visible and unminimized for 332/332 checks and frontmost for 332/332 checks. The highest upper bound on overlap from other windows was 0.0%.

| Launch | CPU, one core | RSS (MiB) | Footprint (MiB) | Median visible update | Max overlap |
|---|---:|---:|---:|---:|---:|
| Previous 500 ms release | 2.60% | 107.40 | 68.60 | 508 ms | 0.0% |
| 2.1.0 A | 2.58% | 92.71 | 31.00 | 506 ms | 0.0% |
| 2.1.0 B | 2.56% | 95.24 | 30.60 | 503 ms | 0.0% |
| 2.1.0 C | 2.74% | 92.72 | 30.55 | 504 ms | 0.0% |

Window-only pixel captures started after each resource trial. The pooled median time between visible changes on 2.1.0 was 504 ms. Sorting by CPU and Name took 4.19 to 4.70 ms of app-side work across the 6 measured release actions, including drawing submission. The two control sorts took 1.05 and 1.13 ms.

The original 2-second version's archived CPU reference was 3.05%. An earlier 500 ms build supplied the separate 92.79 MiB warmed RSS target. The release mean was 0.77 MiB above the target, with 2 of 3 trial means below it. The sections below preserve those earlier test setups. A separate early native-menu experiment averaged 94.15 MiB resident and 2.48% CPU in 3 launches. Deferring creation of the Run text field averaged 94.67 MiB and 2.26% CPU across three foreground launches. A separate launch with mixed window coverage measured 90.29 MiB.

The release passed 34 Rust tests, and the shared system bridge passed 32 Swift baseline tests. On-screen checks covered all eight pages, sorting, expanding groups, editing search and Run new task, viewing process Properties, pause and manual refresh, resizing, minimizing and restoring. Settings switched the window from normal level 0 to floating level 3 and back to level 0. The window recorder exported 19 full-resolution frames. Forced allocation failures for a table cell and the sidebar each exercised native drawing fallback, and the app continued responding to actions. [Raw readings, visibility checks, frame-change timestamps and executable hashes](benchmarks/warm-resident-memory.json)

## November 24, 2026 efficiency benchmark

The previous and revised 500 ms release builds ran on the same Apple M2 Pro Mac with 10 CPU cores and 16 GB of RAM, running macOS 27.2.1. Each build got three fresh launches with the grouped Processes page sorted by name. The window measured 1120 × 720 points on the built-in Retina display. Each trial collected 20 one-second kernel measurements after eight seconds of warm-up. A test-only helper kept the windows on the active Space. Other apps were running.

| Mean across trials | Original 2 s build, archived | Previous 500 ms build, retested | Revised 500 ms build |
|---|---:|---:|---:|
| CPU, percent of one logical core | 3.05% | 3.55% | 2.52% |
| Resident memory | 150.7 MiB | 118.1 MiB | 103.8 MiB |
| Physical footprint | 105.8 MiB | 76.9 MiB | 64.0 MiB |

The three revised CPU means were 2.59%, 2.55%, 2.40%. A fourth trial for each build used a 60-second warm-up and the same 20 measurements.

| After 60 s of warm-up | Previous 500 ms build | Revised 500 ms build |
|---|---:|---:|
| CPU, percent of one logical core | 3.67% | 2.61% |
| Resident memory | 92.8 MiB | 104.2 MiB |
| Physical footprint | 71.0 MiB | 65.7 MiB |

The Processes and Users pages render the static part of the window into a Retina bitmap. A clipped AppKit view redraws the table when a sample changes. An experiment that put only the numeric columns in their own view averaged 3.30% CPU with a bitmap behind them. The full table view handles changes to process groups and their counts without recreating that bitmap. In the final longer trial, sorting by CPU and then by name took 1.12 ms and 1.33 ms of app-side time, including drawing submission.

Logical CPU readings changed about every 508 ms on Normal and 262 ms on High in four-second observations. On Paused they held for two seconds and changed after a manual refresh. Live checks covered all eight pages, sorting, expansion, search, editing in Run new task, asynchronous process Properties, resize, minimize and restore, and window-only recording. A forced bitmap allocation failure exercised the native drawing fallback. All 32 Rust tests and 32 Swift baseline tests passed. [Raw readings and methodology](benchmarks/500ms-efficiency.json)

## November 2026 refresh update

The original and updated Rust release builds were measured on the same 10-core Apple M2 Pro with 16 GB of RAM, running macOS 27.2.1. The Processes page used a 1120 × 720-point window sorted by name. Each resource trial collected 20 one-second observations after an eight-second warm-up. The default comparison averages two trials per build. Other applications were running.

| Processes page | Original | Updated |
|---|---:|---:|
| Default sample interval | 2 seconds | 500 ms |
| Mean physical footprint at default speed | 105.8 MiB | 79.0 MiB |
| Mean resident memory at default speed | 150.7 MiB | 121.3 MiB |
| Mean CPU at default speed, percent of one core | 3.05% | 3.71% |
| Mean CPU with both builds set to Low (4 seconds), one trial each | 1.79% | 0.81% |

With 653 processes in the original model benchmark and 672 in the updated one, the median name-sorted Processes table rebuild fell from 1.32 ms to 0.46 ms. Details fell from 1.29 ms to 0.34 ms. Each measurement used 100 rebuilds and excludes drawing.

Visible logical CPU readings changed about every 495 ms on Normal and 256 ms on High in four-second observations. On Paused, readings held for the two-second observation and changed after a manual refresh. The eight pages, group expansion, resource sorting and logical CPU charts were exercised in the running app. All 27 Rust tests and 32 Swift baseline tests passed. [Measurements and benchmark context](benchmarks/refresh-comparison.json)

## Original release verification

Tested locally on a 16-inch MacBook Pro (2026), Mac17,8, Apple M5 Pro (18 CPU cores), 48 GB RAM, macOS 26.5.1. The app targets general arm64 and macOS 14+. Other M-series devices and older macOS versions were not physically tested.

## Build and packaging

- Optimized Rust release built successfully; primary build/test scripts and GitHub Actions now use Rust.
- Native AppKit UI and a small C system bridge. `otool -L` shows no Swift runtime or web renderer in the executable.
- Mach-O arm64, minimum OS 14.0; installed in Applications with a valid ad-hoc code signature.
- The existing Dock pin points to the installed app. Icon verification passed for all nine source representations; seven ICNS chunks preserve the original embedded PNG bytes.
- Not Developer ID signed or notarized.

## Automated and manual coverage

24 Rust tests passed: native ABI layout, live sampling, counter deltas/wrap/reset, idle versus unavailable cores, process grouping/cycles, identity protection, filtering, bounded/observed history, power-profile parsing and validation, service command timeout, column identities, sorting, navigation, scrolling bounds, and disposable process termination.

The running Rust app was inspected across Processes, Performance, App history, Startup apps, Users, Details, Services and Settings. Checks included live data, CPU overall/logical views, graph context menus and summary mode, memory composition, resource selection, both directions of CPU/Memory/Disk/Network sorting, search/clear/no-results, service search alignment, navigation collapse, Settings scrolling through refresh, keyboard routes and dialogs. AX command children were exposed and used during verification; this is not a complete VoiceOver audit.

A disposable `TaskManagerTestWorker` was selected by PID in the final Rust interface, ended using its confirmation dialog, and independently verified absent afterward. Existing user applications were not terminated. Generation-tagged inspector results prevent a previous asynchronous response from populating a later Properties dialog.

The [component audit](COMPONENT_AUDIT.md) records remaining differences and untested mutations. The old Swift suite's 32 core tests and 67 integration checks are historical baseline evidence, not Rust test results.

## Responsiveness

A final sample of **38 interactions** at a 1120×720-point window, Normal refresh, and approximately 1,200 running processes produced:

| App-side action + drawing | Count | Median | 95th percentile | Maximum |
|---|---:|---:|---:|---:|
| All sampled interactions | 38 | 2.71 ms | 5.84 ms | 7.20 ms |
| Page navigation | 17 | 2.09 ms | 7.20 ms | 7.20 ms |
| Resource-column sorting | 8 | 3.30 ms | 5.84 ms | 5.84 ms |
| Dialogs, navigation collapse and resource selection | 13 | 2.52 ms | 4.83 ms | 4.83 ms |

**All 38 sampled actions were below 8 ms.** The first Run dialog measured 4.83 ms after pre-initializing AppKit's field editor; before that change it measured 17.73 ms. Earlier Details navigation reached 12.39 ms before hidden-column and visible-cell formatting optimizations. These earlier outliers motivated the final changes; they are not represented as passing results.

[Raw final observations and context](benchmarks/final-interactions.json) are included. Percentiles use the nearest-rank method. Video encoding and app-view recording had finished before this sample; ordinary background applications remained running.

The timed `paint:` observations include action handling and synchronous drawing submission, but exclude input event delivery, compositor scheduling and physical display scanout. They are not input-to-photon measurements. A finite sample cannot guarantee every click stays below 8 ms on every workload or display. Native window management and administrator authorization also involve the operating system.

Rendering is virtualized to visible rows. Numeric cell formatting is deferred until display, fonts/drawing attributes are cached, and icons are loaded outside the click path. Sampling, service inventory, inspector commands and power changes run off the UI thread. Recording was disabled for interaction measurements.

## Resource use

A 20-second Processes-page observation with Normal refresh and approximately 1,150 processes averaged **3.40% of one CPU core** and **171.6 MiB RSS** on a Rust release candidate. A separately measured Swift baseline averaged **8.69% of one core** and **314.8 MiB RSS**. Background workload and exact execution times differed, so these are exploratory observations, not a controlled speedup claim or a guarantee. The final release adds further deferred numeric formatting; this resource observation predates that last optimization.

## Per-core and power verification

An independent Mach `host_processor_info(PROCESSOR_CPU_LOAD_INFO)` reader reproduced the low CPU 6–11 values while their idle tick counters advanced. During a bounded four-second 18-thread workload, every core reached approximately 99.8–100% busy. Low graphs reflect scheduler activity, not missing cores; no core class is inferred from an index.

The preceding Swift implementation completed an administrator-authorized AC High Power → Low Power → High Power round trip, leaving Battery Automatic unchanged. The Rust controller implements the same source-scoped `pmset` mapping with validation/readback; parsing and restoration logic are tested, and the Rust confirmation/cancel path was checked. The privileged write was not repeated after the rewrite. Final readback remained **AC powermode 2; Battery powermode 0**. No persistent privileged helper, root monitoring, SIP change, or Full Disk Access requirement was introduced.

## Visual and functional limits

The original app icon pixels are verified. Complete pixel identity of the whole application is not certified. Font rasterization, some custom glyphs, native text editing/authorization and OS behavior differ. GPU utilization, per-process networking, Windows affinity/dumps and other unavailable data are not invented. See [PARITY.md](PARITY.md).

The promotional video contains real app-view frames, edited and composited over Apple's default macOS Tahoe wallpaper. Its opening animation and 60-fps composition are presentation, not a latency or frame-rate benchmark.
