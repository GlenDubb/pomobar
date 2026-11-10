# Pomobar

A lightweight focus timer for Windows that keeps the time left in view on your
taskbar. Work in focused sessions with short breaks in between, and a longer
break every few sessions. Native Windows, written in Rust: one small `.exe`,
no runtime to install, no network access.

By [Williams Automation](https://github.com/GlenDubb).

## Features

- **Focus, Short break and Long break** sessions (25 / 5 / 15 minutes by
  default), with a longer break after every 4 focus sessions.
- **The timer on your taskbar.** While a session runs, Pomobar's taskbar button
  shows a progress bar and a badge with the minutes left. The window title (also
  the taskbar label, if you show labels) is the time left, e.g. `18:42 – Focus`.
  Optionally, the minutes left also show in the notification area.
- **Silent alerts.** When a session ends, choose one of:
  - play a chime,
  - **silent:** pop up a window that takes focus, so you notice without any sound,
  - both.
- **Mini timer.** Minimise Pomobar and a small always-on-top `mm:ss` pill appears just
  above the taskbar. Click it to bring the window back, use its button to pause, or drag
  it to any edge (it snaps there and remembers). It steps aside for full-screen apps.
- Taskbar thumbnail buttons to start, pause, skip or reset without opening the window.
- Auto-start breaks and/or focus sessions, if you like.
- Remembers your settings, window position and today's session count.
- Crisp at any display scaling; works on Windows 10 (1809+) and Windows 11, x64 and ARM64.

## Using Pomobar

| Do this | How |
| --- | --- |
| Start / pause | Click **Start**, press <kbd>Space</kbd> or <kbd>Enter</kbd>, or use the ▶ button in the taskbar thumbnail |
| Skip to the next session | The ⏭ button, or <kbd>N</kbd> |
| Restart this session | The ⟲ button, or <kbd>R</kbd> |
| Switch session type | Click a tab, or <kbd>1</kbd> / <kbd>2</kbd> / <kbd>3</kbd> |
| Settings | The ⚙ button, or <kbd>Ctrl</kbd>+<kbd>,</kbd> |

The **#n** at the bottom is today's focus session number, and the dots show how
many sessions are left until the long break.

### Seeing the timer on the taskbar (Windows 11)

- The **badge** and **progress bar** on Pomobar's taskbar button appear by
  themselves. If you don't see the badge, turn on *Settings › Personalization ›
  Taskbar › Taskbar behaviours › Show badges on taskbar apps*.
- To see the full `mm:ss` on the taskbar, set *Combine taskbar buttons and hide
  labels* to **Never** in the same place.
- New notification-area icons start out hidden. To keep Pomobar's visible, turn
  it on under *Settings › Personalization › Taskbar › Other system tray icons*,
  or drag it out of the ^ overflow.

Settings are saved in `%LOCALAPPDATA%\Pomobar\settings.ini`.

## Building from source

You need Windows, [Rust](https://rustup.rs) and the Visual Studio C++ build
tools with a Windows SDK (the Rust installer offers to set these up).

```powershell
cargo run               # debug build
cargo run -- --demo     # demo mode: "minutes" become seconds, pop-up alerts on
cargo test              # unit tests for the timer and settings logic
cargo build --release   # target\release\pomobar.exe
```

### Packaging for the Microsoft Store

```powershell
rustup target add aarch64-pc-windows-msvc   # once, for the ARM64 build
./packaging/build-msix.ps1                  # writes dist\Pomobar_<version>.msixbundle
```

GitHub Actions runs this on every push, so you can also download the bundle
from the latest run's **pomobar-store-package** artifact. The package isn't
signed; the Microsoft Store signs it when you submit. See
[docs/PUBLISHING.md](docs/PUBLISHING.md) for the full publishing walkthrough.

### Testing the packaged app

To run Pomobar the way the Store installs it, turn on Developer Mode (*Settings
› System › For developers*), build the package, then register the unpacked
layout (no certificate needed):

```powershell
./packaging/build-msix.ps1 -Arch x64
Add-AppxPackage -Register dist\stage-x64\AppxManifest.xml
```

Remove it again with `Get-AppxPackage *Pomobar* | Remove-AppxPackage`.

## How it's built

- `src/timer.rs`, `src/settings.rs`, `src/chime.rs`: plain Rust, unit-tested on any OS.
- `src/win/`: the Windows side, using Microsoft's [`windows`](https://crates.io/crates/windows)
  crate: Direct2D/DirectWrite drawing, the taskbar (`ITaskbarList3`), the
  notification area, the focus-taking pop-up, and a Settings window built from
  standard controls.
- `packaging/`: the MSIX manifest, logos and build script. `tools/make_icons.py`
  regenerates every icon size.

## Privacy

Pomobar collects nothing and never goes online. See [PRIVACY.md](PRIVACY.md).

## Licence

[MIT](LICENSE) © 2026 Williams Automation.

_Pomodoro® and the Pomodoro Technique® are trademarks of Francesco Cirillo.
Pomobar is not affiliated with, or endorsed by, Francesco Cirillo._
