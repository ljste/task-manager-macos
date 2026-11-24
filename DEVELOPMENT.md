# Development and measurement

`./test.sh` runs Rust unit/regression tests, including a live sampler and a disposable process identity/termination test. `./build.sh` builds the release app. The Rust and Swift builds share the C system bridge.

The Rust UI draws the Processes and Users tables at the window’s backing scale with Core Graphics. Once the table has a complete image, the window keeps separate images for the toolbar and sidebar and releases the full-window image. Sorting replaces the table image. Between sorts, changed headings and cells receive small images at their own coordinates. A full table update releases those images. AppKit handles drawing if a bitmap allocation fails. Image I/O decodes application icons at their display size.

The app writes bounded timing observations to `~/Library/Application Support/TaskManager/rust-timings.json` approximately every 30 seconds. Restart to begin a new interaction buffer. Entries named `paint:<action>` include action handling, synchronous view drawing submission, and the accessibility notification. Plain action entries omit drawing. These measurements exclude input delivery and display scanout; they cannot establish input-to-photon latency.

To reproduce a basic interaction sample, launch a release build, allow initial sampling/icon loading to settle, visit all eight pages, click resource headers in both directions, and exercise search and menus. Record the process count, refresh interval, window size, hardware, concurrent workload, and warm/cold status. Keep the recorder off during measurements. Native window management and administrative operations may involve OS work beyond the application handler.

`rust/target/release/task-manager --bench-model` reports model rebuilding separately from rendering. `--sample` performs a one-shot native sample. Neither is a full UI responsiveness benchmark.

For a 20-second resource measurement of a running release, build the included probe and pass the app's PID. Each observation reports CPU as a percentage of one core, resident memory and physical footprint in MiB.

```sh
clang -O2 Scripts/MeasureProcess.c Sources/SystemBridge/SystemBridge.c \
  -ISources/SystemBridge/include -framework IOKit -framework CoreFoundation \
  -o /tmp/task-manager-measure
/tmp/task-manager-measure "$(pgrep -x TaskManagerRust)"
```

Use one running copy of the app for this command. Record the page, sort, window size and refresh setting when comparing builds.

For explicitly enabled demo capture, create `demo-export-path.txt` in the application's support directory containing an absolute output directory, then press Command-Shift-R in the app to start/stop. It exports only the application's own view as timestamped PNG frames at approximately 10 fps, with a bounded queue and a 1,800-frame cap. It does not record other apps or the display. Remove the configuration file when finished. The feature is inactive by default.
