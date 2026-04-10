# dloop

> A terminal UI for Android development — build, install, run, and stream logcat without leaving your keyboard.

```
┌ Devices ──────────┐ Logcat ──────────────────────────────────────────────────────────────────────────────
│ ▶ Pixel 9         │ ● LIVE  streaming  [all]
│   XXXXXXX…        │ no filter
│                   │
│                   │ 15:43:12  OkHttp              I   --> GET https://api.example.com/v1/health
├ Build ▸ ──────────│ 15:43:12  OkHttp              I   <-- 200 OK (123ms)
│ CanaryDevDebug    │ 15:43:13  HealthViewModel      I   health_state_changed: {status=OK}
│                   │ 15:43:13  HealthViewModel      I   fleet_connection: true
└───────────────────┘ 15:43:14  Analytics            D   Event: screen_view {screen=Home}
────────────────────────────────────────────────────────────────────────────────────────────────────────────
 Tab panes   b build   i install   n run   v variant   l logcat   a all/pkg   f filter   e expand   q quit
```

Replace Android Studio's run/debug panel with a fast TUI that sits beside your editor.

## Features

- **Device management** — lists connected ADB devices; shows Android version, API level, and battery when available
- **Gradle inference** — detects `applicationId`, `productFlavors`, and `flavorDimensions` from `app/build.gradle(.kts)`; picks the right `assemble<Variant>Debug` / `install<Variant>Debug` task automatically
- **Multi-dimension flavors** — handles `canaryDevDebug`, `stableProdRelease`-style variants across multiple flavor dimensions
- **Build & Install** — spawns Gradle as a subprocess, streams output live; expandable build pane; **build history** overlay (`H`)
- **Run** — install + auto-launch the app with one keystroke
- **Variant picker** — floating overlay to switch build variant without editing config files
- **Logcat streaming** — live `adb logcat` with rustycat-style rendering: 23-char tag column, tag repeat suppression, word-wrapped messages
- **Crash / ANR highlighting** — detected crash blocks get a red highlight; count in the info bar; **y** copies the last crash to the clipboard
- **Scrollable log** — scroll back through history with `↑`/`↓` or `j`/`k`, `End` to return to tail
- **Filter** — live text filter across tag + message; **exclude** filter (`x`) and config `exclude_filters` to drop noisy lines
- **Export** — **w** writes the current (pane-filtered) log buffer to `dloop-<unix_ts>.log` in the project root
- **scrcpy** — launch screen mirroring for the selected device
- **Per-project config** — `.loopcat.toml` overrides for packages, tasks, log level, scrcpy args, exclude patterns
- **`dloop init`** — scaffold `.loopcat.toml` from Gradle inference

## Requirements

| Tool | Required | Notes |
|------|----------|-------|
| Rust 1.74+ | Build only | via [rustup](https://rustup.rs) |
| `adb` | Yes | Android SDK Platform Tools |
| `gradlew` | For build/install | Any standard Android project |
| `scrcpy` | No | For `m` screen mirror action |

## Installation

Binary name: **`dloop`**

```bash
# Cargo (installs to ~/.cargo/bin)
make install

# Homebrew (after publishing release artifacts; tap is cesarferreira/tap)
brew install cesarferreira/tap/dloop

# User-local (installs to ~/.local/bin)
make install-user

# System-wide (installs to /usr/local/bin, requires sudo)
make install-system
```

To uninstall: `make uninstall`, `make uninstall-user`, or `make uninstall-system`.

Release tarballs and checksums for Homebrew: `make release` (requires optional `rustup target add` for cross-compiles).

## Usage

Run from your Android project root:

```bash
dloop
# or point at a project
dloop --project /path/to/my/android/app
```

Scaffold config without starting the TUI:

```bash
dloop init
dloop init --project /path/to/android/project
```

dloop opens immediately. If a device is connected, logcat starts automatically.

## Keybindings

| Key | Action |
|-----|--------|
| `b` | Build (assemble only) |
| `i` | Install (assemble + install) |
| `n` | **Run** — install then launch the app |
| `v` | Open variant picker |
| `l` | Toggle logcat on/off |
| `a` | Toggle all-logs ↔ package-filter mode |
| `f` | Open/close **include** filter input |
| `x` | Open/close **exclude** filter input |
| `w` | Export visible log lines to `dloop-<timestamp>.log` |
| `y` | Copy last captured crash block to clipboard (`pbcopy` / `xclip` / `wl-copy`) |
| `H` / `h` | Open/close build history overlay |
| `Space` | Pause / resume log streaming |
| `↑` / `↓` | Scroll logcat (when Logs pane active) or navigate devices |
| `j` / `k` | Same as ↑/↓ (vim style) |
| `PageUp` / `PageDown` | Scroll logcat 20 lines |
| `End` / `G` | Jump to tail (live) |
| `e` | Expand / collapse build output |
| `c` | Clear log buffer |
| `m` | Launch scrcpy |
| `s` | Stop current Gradle / logcat process |
| `r` | Refresh device list |
| `d` | Device picker |
| `p` | Package filter picker |
| `q` | Quit |

In filter / exclude mode: type to edit, `Enter` or `Esc` to close.  
In variant picker: `↑`/`↓` to move, `Enter` to select, `Esc` to cancel.

## Gradle Inference

dloop reads `app/build.gradle` (or `.kts`) on startup and infers everything it can:

```groovy
android {
    defaultConfig {
        applicationId "ai.example.app"   // → base package for logcat filter
    }
    flavorDimensions "track", "environment"
    productFlavors {
        canary { dimension "track" }
        stable { dimension "track" }
        dev {
            dimension "environment"
            applicationIdSuffix ".dev"   // → "ai.example.app.dev"
        }
        prod { dimension "environment" }
    }
}
```

Result: default variant **`canaryDevDebug`**, tasks **`assembleCanaryDevDebug`** / **`installCanaryDevDebug`**, packages `["ai.example.app", "ai.example.app.dev"]`.

Use the variant picker (`v`) to switch at runtime, or override in `.loopcat.toml`.

## Configuration

| File | Purpose |
|------|---------|
| `~/.config/droid-loop/config.toml` | Global: preferred device serial, default log level |
| `.loopcat.toml` or `.droid-loop.toml` | Per-project overrides |

**`.loopcat.toml` example:**

```toml
# Explicit package list (skips inference)
packages = ["com.example.app", "com.example.app.dev"]

# Override inferred Gradle tasks
assemble_task = "assembleCanaryDevDebug"
install_task  = "installCanaryDevDebug"

# Logcat
log_level   = "D"
log_filters = ["OkHttp", "MyApp"]
exclude_filters = ["chatty", "ViewRootImpl"]

# scrcpy extra flags
scrcpy_args = ["--window-title", "MyApp Mirror"]
```

## Related

Built on top of patterns from:

- [rustycat](https://github.com/cesarferreira/rustycat) — logcat rendering style and parsing
- [dab](https://github.com/cesarferreira/dab) — ADB client helpers

## License

MIT
