# Agent Instructions

## Release builds

Every time you do a release build (`npm run build`):

1. **Increment the version number** before building. Bump the patch version (e.g. `0.1.1` → `0.1.2`) unless told otherwise, and keep these in sync:
   - `src-tauri/tauri.conf.json` (`version`, shown in the tray menu's About item)
   - `package.json` (`version`)
   - `src-tauri/Cargo.toml` (`[package] version`)
2. **If the app is already running, stop it** before building, without asking:
   ```powershell
   Stop-Process -Name rove-stickynotes -Force -ErrorAction SilentlyContinue
   ```
3. **Do the build**: `npm run build`
4. **Restart the app** after a successful build, if you stopped it in step 2:
   ```powershell
   Start-Process "src-tauri\target\release\rove-stickynotes.exe"
   ```

## Installers

Never create an installer (MSI, NSIS, etc.) unless explicitly asked. Release builds use `tauri build --no-bundle`, which produces only the executable. To build installers when asked, run `npx tauri build` (bundling is configured in `src-tauri/tauri.conf.json`).
