# Agent Instructions

## Git

Commit and push all changes directly to `main`, not a feature branch, unless the user says otherwise. The user often builds on a Mac by pulling the source, so changes need to be pushed to reach it.

## Release builds

Every time you do a release build, on Windows or macOS:

1. **If there are uncommitted changes** (check `git status --porcelain`), **increment the version number** before building. If the working tree is clean, keep the current version and build without bumping, which is the normal case after pulling the source onto a Mac. Bump the patch version (e.g. `0.1.1` → `0.1.2`) unless told otherwise, and keep these in sync:
   - `src-tauri/tauri.conf.json` (`version`, shown in the tray menu's About item)
   - `package.json` (`version`)
   - `package-lock.json` (both the top-level `version` and the one under `packages[""]`)
   - `src-tauri/Cargo.toml` (`[package] version`)
   - `src-tauri/Cargo.lock` (the `version` of the `rove-stickynotes` package)
2. **If the app is already running, stop it** before building, without asking:
   - Windows:
     ```powershell
     Stop-Process -Name rove-stickynotes -Force -ErrorAction SilentlyContinue
     ```
   - macOS:
     ```sh
     pkill -x rove-stickynotes
     ```
3. **Do the build**:
   - Windows: `npm run build` (exe only)
   - macOS: `npm run tauri build -- --bundles app` (app bundle only, no DMG)
4. **Restart the app** after a successful build, if you stopped it in step 2:
   - Windows:
     ```powershell
     Start-Process "src-tauri\target\release\rove-stickynotes.exe"
     ```
   - macOS:
     ```sh
     open "src-tauri/target/release/bundle/macos/Rove StickyNotes.app"
     ```

## Installers

Never create an installer (MSI, NSIS, DMG, etc.) unless explicitly asked. To build installers when asked, run `npx tauri build` (bundling is configured in `src-tauri/tauri.conf.json`).
