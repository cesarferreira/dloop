# Droid Loop TUI (`dloop`)

Terminal UI for Android **build**, **install**, and **logcat** workflows. Orchestrates `adb`, Gradle (`gradlew`), and optionally [scrcpy](https://github.com/Genymobile/scrcpy) — it does not replace the Android SDK.

## Requirements

- Rust 1.74+
- `adb` on `PATH`
- Project with `gradlew` / `gradlew.bat` (for build/install)
- `scrcpy` on `PATH` (optional, for screen mirroring)

## Install

Binary name: **`dloop`**. Pick one:

**1. Cargo (typical for Rust users)** — installs to `~/.cargo/bin` (rustup usually adds this to `PATH`):

```bash
make install
# or: cargo install --path .
```

**2. User-wide without Cargo on PATH** — copies the binary to `~/.local/bin`:

```bash
make install-user
# then ensure ~/.local/bin is on PATH, e.g. in ~/.zshrc:
# export PATH="$HOME/.local/bin:$PATH"
```

**3. System-wide** — installs to `/usr/local/bin` (requires `sudo`; override with `PREFIX=/opt/homebrew` on some Macs):

```bash
make install-system
```

Uninstall: `make uninstall` (Cargo), `make uninstall-user`, or `make uninstall-system`.

## Usage

From an Android project root (or pass `--project /path/to/android`):

```bash
dloop
```

### Inference (no config required)

When you run `dloop` from an Android project, it scans **`app/build.gradle`** or **`app/build.gradle.kts`** for:

- **`applicationId`** and **`applicationIdSuffix`** (per flavor) → builds the list of app IDs (e.g. `ai.wayve.app`, `ai.wayve.app.dev`) for logcat filtering.
- **`productFlavors`** → picks default Gradle tasks **`assemble<Flavor>Debug`** and **`install<Flavor>Debug`** (first flavor after sorting, usually `dev` before `prod`). With no flavors, it uses **`assembleDebug`** / **`installDebug`**.

Override any of this in `.loopcat.toml` if your script layout is unusual.

### Config

| Location | Purpose |
|----------|---------|
| `~/.config/droid-loop/config.toml` | Global defaults (preferred device serial, default log level) |
| `.loopcat.toml` or `.droid-loop.toml` | Per-project: packages, Gradle tasks, log filters, scrcpy args |

Example `.loopcat.toml`:

```toml
package = "com.example.app"
variant = "debug"
assemble_task = "assembleDebug"
install_task = "installDebug"
log_level = "D,E,W"
log_filters = ["OkHttp", "MyTag"]
scrcpy_args = ["--window-title", "MyApp"]
```

### Keybindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `r` | Refresh devices |
| `Tab` / `Shift+Tab` | Next / previous pane |
| `↑` / `↓` | Select device |
| `b` | Run assemble task (default `assembleDebug`) |
| `i` | Run install task (default `installDebug`) |
| `l` | Toggle logcat stream |
| `f` | Toggle filter input (log pane) |
| `c` | Clear log buffer (`adb logcat -c`) |
| `Space` | Pause / resume log streaming (when log pane focused) |
| `m` | Launch scrcpy for selected device |
| `s` | Stop Gradle / logcat subprocess |

In filter mode: type to narrow logs; `Enter` or `Esc` exits filter mode.

## Related projects

Inspired by and aligned with [rustycat](https://github.com/cesarferreira/rustycat) (logcat formatting) and [dab](https://github.com/cesarferreira/dab) (ADB helpers).

## License

MIT
