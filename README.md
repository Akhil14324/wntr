# WNTR

A dynamic notch for Windows. A small black pill sits at the top of your screen and grows when you hover it: music with synced lyrics, a file shelf you can drag in and out of, a focus timer, reminders, calendar, notes, clipboard history and more.

Built with Tauri 2 (Rust + WebView2) and React. Free, local-first, no accounts.

## Install

Download from [Releases](https://github.com/Akhil14324/wntr/releases/latest):

- `WNTR_x.y.z_x64-setup.exe` — installer (per-user, no admin)
- `WNTR_x.y.z_portable.exe` — run from anywhere

The EXEs are not code-signed, so SmartScreen warns on first run: **More info → Run anyway**. `SHA256SUMS.txt` lists the checksums.

## Using it

- **Hover** the pill to expand it; move away to collapse. Clicks pass through the empty area around it.
- **Switch pages** with the mouse wheel, ← / →, the dots at the bottom (hover them for icons), or the page letter keys.
- **Ctrl+Alt+N** opens/closes the notch from anywhere (change it in Settings).
- **Tray icon**: Open, Settings, Do Not Disturb, About, Exit.

### Pages

| Page | What it does |
| --- | --- |
| Now | Music, next task, next event, focus timer and water at a glance |
| Music | Any app in Windows media controls (YouTube Music in Chrome/Edge, Spotify, Apple Music, …): art, seek, controls, volume, session picker |
| Lyrics | Synced lyrics from LRCLIB, click a line to seek, per-song offset, manual search, cached offline. The collapsed pill shows the current line |
| Shelf | Drop files, browser images or text on the notch; drag them back out into any app (native drag, like Explorer). Right-click → Copy file pastes with Ctrl+V anywhere |
| Focus | Pomodoro with chimes and stats; countdown in the collapsed pill |
| Water | Quick-add bottles, reminders during active hours, 7-day chart and 30-day heatmap |
| To-do | Due times, colors, priority, repeats, reminders with snooze, undo |
| Calendar | Secret iCal URLs (Google/Outlook/Apple) or .ics files, next-event countdown, alerts, Join button for Meet/Zoom/Teams |
| Notes | Quick capture, full-text search, pin, Markdown export |
| Clipboard | Text and image history, pin, search, pause |
| Habits | Daily check-offs with streaks |
| Launcher | Pin apps, folders and URLs |
| Weather | Open-Meteo forecast for a city you enter |
| Timers | One-off countdown timers |
| Stats | Focus, water and task charts with CSV export |

Status popups show battery/charger, Wi-Fi, Bluetooth devices (with battery where Windows exposes it), Caps Lock and keyboard language changes, plus volume and laptop-brightness HUDs. WNTR reads only the Caps Lock state and the active keyboard layout — never typed keys.

## Privacy

Everything is stored in `%APPDATA%\WNTR` (SQLite + `settings.json`). Network requests go only to LRCLIB (song title/artist/duration), Open-Meteo (your city's coordinates), the calendar URLs you add, and GitHub Releases for update checks.

## Development

Requirements: Node 22+, Rust (stable, MSVC), WebView2 (built into Windows 11).

```powershell
npm install
npm run tauri dev      # run with hot reload
npm run lint; npm run typecheck
cd src-tauri; cargo test; cargo clippy --all-targets
npm run tauri build    # installer in src-tauri/target/release/bundle/nsis
```

Release builds sign the auto-update bundle, so `tauri build` needs `TAURI_SIGNING_PRIVATE_KEY` set (or set `bundle.createUpdaterArtifacts` to `false` locally).

### Releasing

1. Bump `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`.
2. `git tag v0.2.0 && git push --tags`

The Release workflow builds on `windows-latest`, publishes the installer, portable EXE, checksums and the signed `latest.json` the in-app updater reads. Repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` hold the updater key.

## License

MIT
