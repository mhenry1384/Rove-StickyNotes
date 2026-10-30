# Rove StickyNotes

A lightweight sticky notes app built with Tauri 2. It lives only in the Windows system tray / macOS menu bar.

## Usage

- **Double-click the tray icon** (or choose **New Note** from its menu) to create a note.
- Drag a note by its title bar; resize it from any edge. Notes grow as you type, up to about 10 lines, then scroll.
- The **rectangle** button at the top left toggles always-on-top. The **✕** at the top right closes the note.
- **Right-click the tray icon** for the menu: New Note, Manage Closed, About (shows the version), Quit.
- **Manage Closed** lists the last 15 closed notes, newest first. Select one to preview it, then click **Restore Note**.

Open notes, their positions, sizes and on-top state are saved every 10 seconds (if changed) and on quit, and restored
on the next launch. Notes that would be partly or fully off-screen (e.g. a monitor was disconnected) are moved back
onto a monitor. Empty notes are discarded when closed.

Data is stored in `notes.json` in the app data folder:

- Windows: `%APPDATA%\com.rove.stickynotes\`
- macOS: `~/Library/Application Support/com.rove.stickynotes/`

## Development

```sh
npm install
npm run dev     # run with hot reload
npm run build   # release build + installers in src-tauri/target/release/bundle
```

Project layout:

- `src-tauri/src/main.rs` – tray, note windows, persistence, multi-monitor placement
- `note.html`, `src/note.js`, `src/note.css` – a sticky note window
- `closed.html`, `src/closed.js`, `src/closed.css` – the Manage Closed window
