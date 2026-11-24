# Task Manager for macOS

Task Manager for macOS displays live processes and system activity in the style of Windows 11 Task Manager. This independent port uses Rust, native AppKit drawing and a C bridge to macOS monitoring APIs. It builds on [hotredsam’s Task Manager for macOS](https://github.com/hotredsam/task-manager-macos).

The layout follows Windows 11 22H2, with right-hand window controls, eight navigation pages, grouped processes, sortable resource columns, context menus and live performance graphs. The centered search box follows the later Windows 11 design. [Windows 11 visual reference](https://www.windowscentral.com/whats-new-task-manager-windows-11-version-22h2)

![Native Rust Task Manager showing live CPU cores](docs/preview.png)

## Compatibility

Targets **M1 and later M-series Macs on macOS 14 or later**. The build uses general `arm64`, with processor counts and metrics discovered at runtime.

The performance measurements below were taken on a **10-core Apple M2 Pro with 16 GB RAM** running macOS 27.2.1. The original release was tested on a **16-inch MacBook Pro (2026), model Mac17,8, with an 18-core M5 Pro and 48 GB RAM** running macOS 26.5.1, using Rust 1.98.1 and Apple Command Line Tools. Other M-series configurations and older supported macOS releases remain compatibility targets. Intel is outside the build target.

## Download and build

[Download the Apple Silicon app](https://github.com/ljste/task-manager-macos/releases/latest). Extract the ZIP and move Task Manager to Applications. The release has an ad hoc signature and has not been notarized by Apple.

To build from source, install a current stable Rust toolchain, Apple Command Line Tools, and Python 3, then run these commands.

```sh
git clone https://github.com/ljste/task-manager-macos.git
cd task-manager-macos
./test.sh
./build.sh
open "../Task Manager.app"
```

The scripts use locked Cargo dependencies and target macOS 14. To pin the app to the Dock, choose **Options → Keep in Dock** from its Dock menu.

The former Swift implementation remains in `Sources/` and `Tests/` as a historical baseline. `build-swift.sh` and `test-swift.sh` build/test that baseline. The main build and CI use Rust; the Rust binary shares the C `SystemBridge` source with the Swift baseline.

## Using it

- **Processes / Details:** click a header to sort and reverse it; the selected header has shading and a direction chevron. Resize columns by dragging their edges; use View → Select columns to change visibility. Search names, paths, bundle IDs, PIDs, and owners. Expand app groups with their disclosure arrows. Command-click and Shift-click select multiple rows.
- **Process actions:** End task requests termination; More → Force quit terminates immediately. Both confirm and revalidate PID, start time, and owner before signaling. Critical/system/other-user processes and groups containing them are protected. Properties shows identity, path, arguments, open-file count, and available signing information, with scrolling and Copy details.
- **Performance:** choose CPU, Memory, Disk 0, Network, or GPU 0. Right-click CPU → Change graph to → Logical processors. Kernel overlays, graph summary view, and copying are supported. Double-click a main graph to toggle summary; Escape exits it.
- **Efficiency mode:** controls the current power source's actual macOS Low Power Mode for the whole Mac. A confirmation explains this scope; macOS asks for administrator authorization. Turning it off restores the saved previous mode, including High Power where supported. Other power-source profiles are preserved. It is not Windows per-process EcoQoS.
- **App history:** records observed CPU time locally. Show history for all processes is optional. Windows-only network/notification accounting is explicitly unavailable.
- **Startup apps** shows Open at Login items, launch agents and registered background helpers. Select a row and choose **Disable** or **Enable**. Command-click or Shift-click to change several together. Account helpers need no administrator password. For Mac-wide daemons, Task Manager requests administrator approval once for the selected group. Disabling a helper prevents future loads. An already running helper can continue until it exits. Enabling restores its launchd policy, subject to macOS background permission. **Open Login Items** is available if you want to review that separate permission.
- **Services** shows launchd jobs. Start, stop and restart are available for eligible LaunchAgents in your own Library.
- **Users:** expand a user to see their processes. System service accounts are excluded. Disconnect is unavailable.
- **Settings:** start page, update interval, pause, window behavior, grouping, history visibility, graph interval, units, theme, and launch at login. Login registration uses Apple's ServiceManagement API and may need approval in System Settings.

The window uses normal macOS stacking by default. You can enable Always on top in Settings. Normal refresh updates process and system metrics every 500 ms. High updates every 250 ms. Low updates every four seconds. Paused keeps the displayed sample available, and F5 takes a fresh reading. Graphs use elapsed time across speed changes.

Command-1…8 switches pages; Command-F searches; Command-N runs an application; F5 refreshes; Control-Tab cycles pages. Arrow keys navigate/expand rows, Tab moves through commands, Enter activates a focused command or opens properties, Delete requests End task, and Escape dismisses menus/dialogs. Option-Space opens the window menu; Option-F4 closes. Maximize uses the current screen's usable area and restores its previous frame.

## Implementation and data

| Area | Implementation |
|---|---|
| UI | Rust, objc2, native AppKit; custom Windows-style drawing, native text editing, accessible controls |
| Processes | libproc, BSD identity checks, Mach CPU-time conversion, bundle metadata |
| CPU | Mach host and per-processor tick deltas, runtime core counts, sysctl metadata |
| Memory | Host VM counters, resident process memory, swap usage |
| Disk | Published IOKit block-storage totals, permitted process I/O counters, statfs capacity |
| Network | BSD counters for active physical interfaces; aggregate send/receive |
| Services | launchctl inventory and plist metadata; bounded subprocess output/timeouts |
| Power | ProcessInfo state plus administrator-authorized, allowlisted pmset commands and readback |

Sampling and service discovery run on independent workers. Each delivery wakes the UI. Processes and Users reuse stable rows and repaint visible cells when their displayed values or heat colors change. Open menus, confirmations, and drags keep their backing snapshot stable. History is capped at 5,000 entries; graph samples and timing buffers are bounded. Data and preferences stay in `~/Library/Application Support/TaskManager/`; the app does not upload them.

## Performance and verification

On the built-in Retina display of a 10-core M2 Pro, the grouped Processes page showed a visible change every **504 ms** at the median with the default 500 ms refresh. Both releases ran in a foreground, unobstructed 1120 × 720-point window, sorted by Name. Each fresh launch warmed for 60 seconds before 20 one-second kernel measurements.

| Release | Launches | CPU, % of one core | Resident memory | Physical footprint |
|---|---:|---:|---:|---:|
| Previous 500 ms release | 1 | 2.60% | 107.40 MiB | 68.60 MiB |
| **2.1.0** | **3** | **2.63%** | **93.56 MiB** | **30.72 MiB** |

The three 2.1.0 launches averaged 92.71, 95.24 and 92.72 MiB resident. Their combined mean was 13.84 MiB lower than the matched previous release and 0.77 MiB above the earlier 92.79 MiB warmed benchmark. All three CPU means (2.58%, 2.56%, 2.74%) were below the original 2-second version’s 3.05%. [Raw measurements, visibility checks and update times](benchmarks/warm-resident-memory.json)

Sorting by CPU and Name took 4.19 to 4.70 ms across six measured actions in 2.1.0, compared with 1.05 and 1.13 ms in the previous release. Both met the 8 ms target for app-side work, including drawing submission. The release passed 34 Rust tests and 32 Swift baseline tests. On-screen checks covered all eight pages, live updates, text editing, window management, recording and recovery from drawing allocation failures.

## Limitations

macOS supplies the metrics and actions behind the Windows-style interface. Disk and network totals aggregate devices. GPU utilization, per-process network traffic, dynamic CPU clocks and disk active-time percentages are unavailable. Windows-specific features such as affinity masks, NUMA controls and process dumps are also unavailable. Font rendering, native dialogs, window snapping and some table interactions differ from Windows.

## License

The project’s original source code uses the [MIT license](LICENSE). The Windows Task Manager icon and other proprietary Windows artwork retain their third-party rights. The build verifies the icon against the source PNGs. The app uses Segoe UI when available on the Mac and bundles Microsoft’s open-source Selawik as its fallback. [Third-party notices](THIRD_PARTY_NOTICES.txt)
