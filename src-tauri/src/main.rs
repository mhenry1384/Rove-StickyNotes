// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, RunEvent, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const MAX_CLOSED: usize = 15;
const SAVE_INTERVAL: Duration = Duration::from_secs(10);
// Roughly 32 characters wide and one line tall (title bar + one line of text).
const DEFAULT_WIDTH: f64 = 270.0;
const DEFAULT_HEIGHT: f64 = 54.0;
const MIN_WIDTH: f64 = 120.0;
const MIN_HEIGHT: f64 = 48.0;
const MAX_WIDTH: f64 = 10000.0;
const NOTE_PREFIX: &str = "note-";

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Note {
    id: String,
    text: String,
    /// Outer position in physical (virtual desktop) pixels.
    x: i32,
    y: i32,
    /// Size in logical pixels, so it scales correctly across monitors with different DPI.
    width: f64,
    height: f64,
    on_top: bool,
    #[serde(default)]
    closed_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Data {
    notes: Vec<Note>,
    closed: Vec<Note>,
    #[serde(default = "default_true")]
    start_at_login: bool,
}

impl Default for Data {
    fn default() -> Self {
        Data {
            notes: Vec::new(),
            closed: Vec::new(),
            start_at_login: true,
        }
    }
}

fn default_true() -> bool {
    true
}

struct AppState {
    data: Mutex<Data>,
    dirty: AtomicBool,
    quitting: AtomicBool,
    path: PathBuf,
}

impl AppState {
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    fn save(&self) {
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return;
        }
        let json = {
            let data = self.data.lock().unwrap();
            serde_json::to_string_pretty(&*data).unwrap()
        };
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write to a temp file and rename so a crash mid-write can't corrupt the notes.
        let tmp = self.path.with_extension("json.tmp");
        let result = std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, &self.path));
        if let Err(e) = result {
            eprintln!("Failed to save notes: {}", e);
            self.mark_dirty();
        }
    }
}

fn load_data(path: &PathBuf) -> Data {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Data::default();
    };
    match serde_json::from_str(contents.trim_start_matches('\u{feff}')) {
        Ok(data) => data,
        Err(e) => {
            // Keep the unreadable file rather than overwriting it on the next save.
            eprintln!("Failed to read notes ({}); backing up to notes.json.bad", e);
            let _ = std::fs::rename(path, path.with_extension("json.bad"));
            Data::default()
        }
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{}{}", now_millis(), COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn note_id(label: &str) -> Option<&str> {
    label.strip_prefix(NOTE_PREFIX)
}

/// Moves (and if necessary shrinks) a note so that it lies entirely within the work area of a
/// monitor. If it already fits on some monitor it is left alone; otherwise it is placed on the
/// monitor it overlaps the most, falling back to the primary monitor.
fn fit_on_screen(app: &AppHandle, note: &mut Note) {
    let monitors = app.available_monitors().unwrap_or_default();
    if monitors.is_empty() {
        return;
    }

    let rect_for = |scale: f64, note: &Note| {
        let w = (note.width * scale).round() as i64;
        let h = (note.height * scale).round() as i64;
        (note.x as i64, note.y as i64, w, h)
    };
    let area = |m: &tauri::Monitor| {
        let wa = m.work_area();
        (
            wa.position.x as i64,
            wa.position.y as i64,
            wa.size.width as i64,
            wa.size.height as i64,
        )
    };

    let mut best: Option<(&tauri::Monitor, i64)> = None;
    for m in &monitors {
        let (x, y, w, h) = rect_for(m.scale_factor(), note);
        let (ax, ay, aw, ah) = area(m);
        if x >= ax && y >= ay && x + w <= ax + aw && y + h <= ay + ah {
            return;
        }
        let ix = (x + w).min(ax + aw) - x.max(ax);
        let iy = (y + h).min(ay + ah) - y.max(ay);
        let overlap = if ix > 0 && iy > 0 { ix * iy } else { 0 };
        if overlap > 0 && best.map_or(true, |(_, o)| overlap > o) {
            best = Some((m, overlap));
        }
    }

    let primary = app.primary_monitor().ok().flatten();
    let target = match best {
        Some((m, _)) => m.clone(),
        None => primary.unwrap_or_else(|| monitors[0].clone()),
    };
    let scale = target.scale_factor();
    let (ax, ay, aw, ah) = area(&target);

    note.width = note.width.min(aw as f64 / scale).max(MIN_WIDTH);
    note.height = note.height.min(ah as f64 / scale).max(MIN_HEIGHT);
    let (x, y, w, h) = rect_for(scale, note);
    note.x = x.min(ax + aw - w).max(ax) as i32;
    note.y = y.min(ay + ah - h).max(ay) as i32;
}

/// Pins a note's height so it can only be resized horizontally; its height follows its text instead.
fn lock_height(window: &WebviewWindow, height: f64) -> tauri::Result<()> {
    window.set_min_size(Some(LogicalSize::new(MIN_WIDTH, height)))?;
    window.set_max_size(Some(LogicalSize::new(MAX_WIDTH, height)))
}

fn open_note_window(app: &AppHandle, note: &Note) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(
        app,
        format!("{}{}", NOTE_PREFIX, note.id),
        WebviewUrl::App("note.html".into()),
    )
    .title("Sticky Note")
    .inner_size(note.width, note.height)
    .decorations(false)
    // Resizing is done by the note's own edge handles (see note.js), so only the width can change.
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .skip_taskbar(true)
    .always_on_top(note.on_top)
    .visible(false)
    .build()?;
    lock_height(&window, note.height)?;
    window.set_position(PhysicalPosition::new(note.x, note.y))?;
    window.show()?;
    window.set_focus()?;
    Ok(window)
}

fn create_note(app: &AppHandle) {
    let mut note = Note {
        id: new_id(),
        text: String::new(),
        x: 0,
        y: 0,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        on_top: true,
        closed_at: None,
    };
    // Open horizontally near the mouse (i.e. near the tray icon) but vertically centred in the
    // work area of the monitor under the mouse, then pull it fully on screen.
    if let Ok(pos) = app.cursor_position() {
        note.x = pos.x as i32 - 140;
        note.y = pos.y as i32 - 120;
        let monitor = app.available_monitors().unwrap_or_default().into_iter().find(|m| {
            let (p, s) = (m.position(), m.size());
            pos.x >= p.x as f64
                && pos.x < p.x as f64 + s.width as f64
                && pos.y >= p.y as f64
                && pos.y < p.y as f64 + s.height as f64
        });
        if let Some(m) = monitor {
            let wa = m.work_area();
            let h = (note.height * m.scale_factor()).round() as i32;
            note.y = wa.position.y + (wa.size.height as i32 - h) / 2;
        }
    }
    fit_on_screen(app, &mut note);

    let state = app.state::<AppState>();
    state.data.lock().unwrap().notes.push(note.clone());
    state.mark_dirty();
    if let Err(e) = open_note_window(app, &note) {
        eprintln!("Failed to open note: {}", e);
    }
}

fn show_closed_manager(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("closed") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(app, "closed", WebviewUrl::App("closed.html".into()))
        .title("Closed Notes - Rove StickyNotes")
        .inner_size(680.0, 440.0)
        .min_inner_size(420.0, 260.0)
        .skip_taskbar(true)
        .center()
        .build();
    match result {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("Failed to open closed notes window: {}", e),
    }
}

/// Moves an open note to the top of the closed list when its window is closed by the user.
fn on_note_closed(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    if state.quitting.load(Ordering::SeqCst) {
        return;
    }
    {
        let mut data = state.data.lock().unwrap();
        let Some(index) = data.notes.iter().position(|n| n.id == id) else {
            return;
        };
        let mut note = data.notes.remove(index);
        // Empty notes aren't worth keeping in the closed list.
        if !note.text.trim().is_empty() {
            note.closed_at = Some(now_millis());
            data.closed.insert(0, note);
            data.closed.truncate(MAX_CLOSED);
        }
    }
    state.mark_dirty();
    let _ = app.emit("closed-changed", ());
}

/// Raises every open note above other windows, keeping each note's own always-on-top setting.
fn bring_notes_to_front(app: &AppHandle) {
    let notes = app.state::<AppState>().data.lock().unwrap().notes.clone();
    for note in &notes {
        let Some(window) = app.get_webview_window(&format!("{}{}", NOTE_PREFIX, note.id)) else {
            continue;
        };
        // Briefly making a window topmost raises it without needing foreground permission.
        let _ = window.set_always_on_top(true);
        let _ = window.set_always_on_top(note.on_top);
        let _ = window.set_focus();
    }
}

/// Registers or unregisters the app as a login item (Windows Run key / macOS LaunchAgent).
/// Skipped in debug builds so development runs don't point the login item at the debug exe.
fn apply_start_at_login(app: &AppHandle, enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let autolaunch = app.autolaunch();
    // Re-enabling on every launch keeps the registered path current if the exe has moved.
    let result = if enabled {
        autolaunch.enable()
    } else if autolaunch.is_enabled().unwrap_or(false) {
        autolaunch.disable()
    } else {
        Ok(())
    };
    if let Err(e) = result {
        eprintln!("Failed to update start at login: {}", e);
    }
}

fn toggle_start_at_login(app: &AppHandle, item: &CheckMenuItem<tauri::Wry>) {
    let state = app.state::<AppState>();
    let enabled = {
        let mut data = state.data.lock().unwrap();
        data.start_at_login = !data.start_at_login;
        data.start_at_login
    };
    state.mark_dirty();
    let _ = item.set_checked(enabled);
    apply_start_at_login(app, enabled);
}

fn quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.quitting.store(true, Ordering::SeqCst);
    state.save();
    app.exit(0);
}

#[tauri::command]
fn get_note(id: String, state: tauri::State<AppState>) -> Option<Note> {
    let data = state.data.lock().unwrap();
    data.notes.iter().find(|n| n.id == id).cloned()
}

#[tauri::command]
fn update_note_text(id: String, text: String, state: tauri::State<AppState>) {
    let mut data = state.data.lock().unwrap();
    if let Some(note) = data.notes.iter_mut().find(|n| n.id == id) {
        if note.text != text {
            note.text = text;
            state.mark_dirty();
        }
    }
}

#[tauri::command]
fn set_on_top(window: WebviewWindow, on_top: bool, state: tauri::State<AppState>) {
    let _ = window.set_always_on_top(on_top);
    let Some(id) = note_id(window.label()) else {
        return;
    };
    let mut data = state.data.lock().unwrap();
    if let Some(note) = data.notes.iter_mut().find(|n| n.id == id) {
        note.on_top = on_top;
        state.mark_dirty();
    }
}

/// Resizes a note to the given logical height (to fit its text), keeping it on screen.
#[tauri::command]
fn fit_note_height(window: WebviewWindow, height: f64, state: tauri::State<AppState>) {
    let Some(id) = note_id(window.label()) else {
        return;
    };
    let mut note = {
        let data = state.data.lock().unwrap();
        match data.notes.iter().find(|n| n.id == id) {
            Some(note) => note.clone(),
            None => return,
        }
    };
    let height = height.max(MIN_HEIGHT);
    if (height - note.height).abs() < 0.5 {
        return;
    }
    note.height = height;
    fit_on_screen(window.app_handle(), &mut note);
    let _ = lock_height(&window, note.height);
    let _ = window.set_size(LogicalSize::new(note.width, note.height));
    let _ = window.set_position(PhysicalPosition::new(note.x, note.y));
}

/// Sets a note's width from its edge handles. When dragging the left edge, `right` is the
/// logical x of the right edge, which stays put while the left edge moves.
#[tauri::command]
fn resize_note_width(window: WebviewWindow, width: f64, right: Option<f64>) {
    let width = width.max(MIN_WIDTH);
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(size) = window.inner_size() else {
        return;
    };
    let height = size.to_logical::<f64>(scale).height;
    let _ = window.set_size(LogicalSize::new(width, height));
    if let (Some(right), Ok(pos)) = (right, window.outer_position()) {
        let y = pos.to_logical::<f64>(scale).y;
        let _ = window.set_position(LogicalPosition::new(right - width, y));
    }
}

#[tauri::command]
fn get_closed(state: tauri::State<AppState>) -> Vec<Note> {
    state.data.lock().unwrap().closed.clone()
}

// Async so the new window isn't created on the main thread while it is blocked (deadlocks on Windows).
#[tauri::command]
async fn restore_note(id: String, app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut note = {
        let mut data = state.data.lock().unwrap();
        let index = data
            .closed
            .iter()
            .position(|n| n.id == id)
            .ok_or("Note not found")?;
        data.closed.remove(index)
    };
    note.closed_at = None;
    fit_on_screen(&app, &mut note);
    state.data.lock().unwrap().notes.push(note.clone());
    state.mark_dirty();
    let _ = app.emit("closed-changed", ());
    open_note_window(&app, &note).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch just creates a new note in the running instance.
            create_note(app);
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            get_note,
            update_note_text,
            set_on_top,
            fit_note_height,
            resize_note_width,
            get_closed,
            restore_note
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let path = app.path().app_data_dir()?.join("notes.json");
            let data = load_data(&path);
            app.manage(AppState {
                data: Mutex::new(data),
                dirty: AtomicBool::new(false),
                quitting: AtomicBool::new(false),
                path,
            });

            let handle = app.handle().clone();

            // Restore the notes that were open last time.
            let notes = {
                let state = handle.state::<AppState>();
                let mut data = state.data.lock().unwrap();
                for note in data.notes.iter_mut() {
                    fit_on_screen(&handle, note);
                }
                data.notes.clone()
            };
            for note in &notes {
                if let Err(e) = open_note_window(&handle, note) {
                    eprintln!("Failed to restore note: {}", e);
                }
            }

            let start_at_login = handle.state::<AppState>().data.lock().unwrap().start_at_login;
            apply_start_at_login(&handle, start_at_login);

            let version = app.package_info().version.to_string();
            let new_item = MenuItem::with_id(app, "new", "New Note", true, None::<&str>)?;
            let about_item = MenuItem::with_id(
                app,
                "about",
                format!("About Rove Sticky Notes {}", version),
                true,
                None::<&str>,
            )?;
            let closed_item =
                MenuItem::with_id(app, "closed", "Manage Closed", true, None::<&str>)?;
            let login_item = CheckMenuItem::with_id(
                app,
                "login",
                "Start at Login",
                true,
                start_at_login,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &new_item,
                    &closed_item,
                    &PredefinedMenuItem::separator(app)?,
                    &login_item,
                    &about_item,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?;

            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Rove StickyNotes")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "new" => create_note(app),
                    "closed" => show_closed_manager(app),
                    "login" => toggle_start_at_login(app, &login_item),
                    "quit" => quit(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => bring_notes_to_front(tray.app_handle()),
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => create_note(tray.app_handle()),
                    _ => {}
                })
                .build(app)?;

            // Save every 10 seconds if anything changed.
            std::thread::spawn(move || loop {
                std::thread::sleep(SAVE_INTERVAL);
                handle.state::<AppState>().save();
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            let Some(id) = note_id(window.label()).map(str::to_string) else {
                return;
            };
            let app = window.app_handle();
            let state = app.state::<AppState>();
            match event {
                WindowEvent::Moved(pos) => {
                    // Ignore the bogus off-screen position Windows reports for minimized windows.
                    if window.is_minimized().unwrap_or(false) {
                        return;
                    }
                    let mut data = state.data.lock().unwrap();
                    if let Some(note) = data.notes.iter_mut().find(|n| n.id == id) {
                        if note.x != pos.x || note.y != pos.y {
                            note.x = pos.x;
                            note.y = pos.y;
                            state.mark_dirty();
                        }
                    }
                }
                WindowEvent::Resized(size) => {
                    if size.width == 0 || size.height == 0 {
                        return;
                    }
                    let scale = window.scale_factor().unwrap_or(1.0);
                    let logical: LogicalSize<f64> = size.to_logical(scale);
                    let mut data = state.data.lock().unwrap();
                    if let Some(note) = data.notes.iter_mut().find(|n| n.id == id) {
                        if (note.width - logical.width).abs() > 0.5
                            || (note.height - logical.height).abs() > 0.5
                        {
                            note.width = logical.width;
                            note.height = logical.height;
                            state.mark_dirty();
                        }
                    }
                }
                WindowEvent::CloseRequested { .. } => on_note_closed(app, &id),
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // Closing the last note must not quit the app; it lives in the tray.
            RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            RunEvent::Exit => {
                let state = app.state::<AppState>();
                state.quitting.store(true, Ordering::SeqCst);
                state.save();
            }
            _ => {}
        });
}
