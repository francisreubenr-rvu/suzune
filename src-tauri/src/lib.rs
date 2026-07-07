mod coordinator;
mod models;
mod personalization;
mod settings;

use coordinator::{Command, Coordinator};
use settings::Settings;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Live app settings, shared between the shortcut handler, the settings
/// window, and the config directory on disk. Behind a Mutex so the
/// settings UI can edit them at runtime.
struct SettingsState {
    settings: Mutex<Settings>,
    config_dir: std::path::PathBuf,
    /// Continuous-mode recording flag, shared across shortcut
    /// re-registrations so a mode change mid-session can't strand it.
    toggle_recording: Arc<AtomicBool>,
}

/// One recent dictation result, kept only in memory unless the user
/// actively corrects it (see `submit_correction`). Cleared on app restart.
#[derive(Clone, serde::Serialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub raw: String,
    pub cleaned: String,
    pub ts: u64,
    /// Tone setting active when this entry was produced; copied into
    /// `CorrectionRecord` if the user later corrects it.
    pub tone: String,
}

/// Shared handle for the personalization feature: the rolling in-memory
/// history buffer (Tauri-managed for the Settings window's commands, and
/// cloned into the coordinator's `Worker` so it can push new entries), plus
/// the config directory the corrections/vocabulary files live under.
/// Cloning is cheap (two `Arc`s and a `PathBuf`) — the same handle is
/// shared, not duplicated data.
#[derive(Clone)]
pub struct HistoryState {
    pub buffer: Arc<Mutex<VecDeque<HistoryEntry>>>,
    pub next_id: Arc<AtomicU64>,
    pub config_dir: std::path::PathBuf,
}

#[tauri::command]
fn get_settings(state: tauri::State<SettingsState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

/// Persist edited settings, apply the shortcut/mode live, and tell the
/// coordinator to reload the rest. Returns an error string the UI shows.
#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: tauri::State<SettingsState>,
    new_settings: Settings,
) -> Result<(), String> {
    // Validate the shortcut before committing anything.
    let _: tauri_plugin_global_shortcut::Shortcut = new_settings
        .shortcut
        .parse()
        .map_err(|e| format!("Invalid shortcut '{}': {e}", new_settings.shortcut))?;

    new_settings
        .save(&state.config_dir)
        .map_err(|e| format!("Could not save settings: {e}"))?;
    *state.settings.lock().unwrap() = new_settings.clone();

    register_shortcut(&app, &new_settings.shortcut)
        .map_err(|e| format!("Could not register shortcut: {e}"))?;

    if app.try_state::<Coordinator>().is_some() {
        app.state::<Coordinator>()
            .send(Command::ReloadSettings(Box::new(new_settings)));
    }
    Ok(())
}

/// List available input-device names so the settings UI can offer a picker.
#[tauri::command]
fn list_input_devices() -> Vec<String> {
    suzune_audio::input_device_names()
}

/// The rolling recent-dictation history (empty unless personalization is
/// enabled — see `HistoryState`'s doc comment). Never touches disk.
#[tauri::command]
fn get_recent_history(state: tauri::State<HistoryState>) -> Vec<HistoryEntry> {
    state
        .buffer
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .cloned()
        .collect()
}

/// Collapse `\r`/`\n` runs into a single space. Closes a prompt-injection
/// surface at the persistence boundary: corrections.jsonl fields are later
/// embedded verbatim into a few-shot prompt block as
/// `"\nInput: {}\nOutput: {}"`, so an embedded newline could fabricate a
/// fake example turn.
fn normalize_single_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_newline = false;
    for ch in s.chars() {
        if ch == '\r' || ch == '\n' {
            if !last_was_newline {
                out.push(' ');
            }
            last_was_newline = true;
        } else {
            out.push(ch);
            last_was_newline = false;
        }
    }
    out.trim().to_string()
}

/// The user has flagged a history entry as wrong and typed the correct
/// version. Persists the (raw, cleaned, corrected) triple to
/// `corrections.jsonl` and asks the running coordinator to pick it up.
#[tauri::command]
fn submit_correction(
    state: tauri::State<HistoryState>,
    app: AppHandle,
    history_id: u64,
    corrected_text: String,
) -> Result<(), String> {
    let entry = {
        let buf = state.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buf.iter().find(|e| e.id == history_id).cloned()
    }
    .ok_or_else(|| {
        "that history entry is no longer available (it may have scrolled past the 50-entry limit)"
            .to_string()
    })?;

    let corrected = normalize_single_line(&corrected_text);
    if corrected.is_empty() {
        return Err("correction text can't be empty".to_string());
    }

    // corrections.jsonl ids are session-independent (unlike HistoryEntry's
    // AtomicU64, which resets to 1 every launch) — derive the next one from
    // what's actually on disk so ids stay unique across restarts.
    let existing = personalization::load_corrections(&state.config_dir);
    let next_id = existing.iter().map(|r| r.id).max().unwrap_or(0) + 1;

    let record = personalization::CorrectionRecord {
        id: next_id,
        ts: personalization::now_unix(),
        raw: normalize_single_line(&entry.raw),
        cleaned: normalize_single_line(&entry.cleaned),
        corrected,
        tone: entry.tone,
    };
    personalization::append_correction(&state.config_dir, &record)
        .map_err(|e| format!("could not save correction: {e}"))?;

    if app.try_state::<Coordinator>().is_some() {
        app.state::<Coordinator>().send(Command::ReloadCorrections);
    }
    Ok(())
}

/// Every correction the user has ever submitted — the user-inspectable
/// view of `corrections.jsonl`.
#[tauri::command]
fn list_corrections(state: tauri::State<HistoryState>) -> Vec<personalization::CorrectionRecord> {
    personalization::load_corrections(&state.config_dir)
}

/// Delete the corrections store and derived vocabulary entirely.
#[tauri::command]
fn clear_corrections(state: tauri::State<HistoryState>, app: AppHandle) -> Result<(), String> {
    personalization::clear_corrections(&state.config_dir)
        .map_err(|e| format!("could not clear corrections: {e}"))?;
    if app.try_state::<Coordinator>().is_some() {
        app.state::<Coordinator>().send(Command::ReloadCorrections);
    }
    Ok(())
}

#[tauri::command]
fn cancel_dictation(state: tauri::State<SettingsState>, app: AppHandle) {
    // The coordinator may not exist yet during first-run download.
    if app.try_state::<Coordinator>().is_some() {
        app.state::<Coordinator>().send(Command::Cancel);
    }
    let _ = state; // keep signature stable for the UI
}

/// True while the app is still fetching first-run models (the setup UI
/// polls this on mount to decide whether to show the download screen).
#[tauri::command]
fn needs_setup(state: tauri::State<SettingsState>, app: AppHandle) -> bool {
    if app.try_state::<Coordinator>().is_some() {
        return false; // engine already running
    }
    let s = state.settings.lock().unwrap();
    !models::models_present(&s.models_root, &s.cleanup_model)
}

/// Start the dictation engine and register the global shortcut. Called once
/// the models are present — immediately at launch, or after the first-run
/// download completes.
fn finish_startup(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    let history = app.state::<HistoryState>().inner().clone();
    let coordinator = Coordinator::start(app.clone(), settings.clone(), history);
    app.manage(coordinator);
    register_shortcut(app, &settings.shortcut)?;
    log::info!("dictation engine started");
    Ok(())
}

/// (Re)register the global dictation shortcut. Unregisters any previous
/// binding first, then installs a handler that reads the current mode from
/// shared state on each key event — so both the hotkey and push-to-talk vs
/// continuous mode can change at runtime without an app restart.
fn register_shortcut(app: &AppHandle, shortcut_str: &str) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::Shortcut;
    let shortcut: Shortcut = shortcut_str
        .parse()
        .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!("parse shortcut: {e}")))?;

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();

    gs.on_shortcut(shortcut, move |app, _shortcut, event| {
        let state = app.state::<SettingsState>();
        let push_to_talk = state.settings.lock().unwrap().push_to_talk;
        let coordinator = app.state::<Coordinator>();
        if push_to_talk {
            match event.state() {
                ShortcutState::Pressed => coordinator.send(Command::StartRecording),
                ShortcutState::Released => coordinator.send(Command::StopAndProcess),
            }
        } else if event.state() == ShortcutState::Pressed {
            // Continuous mode: each press toggles recording on/off.
            let now_recording = !state.toggle_recording.fetch_xor(true, Ordering::SeqCst);
            coordinator.send(if now_recording {
                Command::StartRecording
            } else {
                Command::StopAndProcess
            });
        }
    })
    .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!("register shortcut: {e}")))?;
    Ok(())
}

/// Small always-on-top pill, bottom-center of the primary monitor.
/// Hidden while idle; its own JS shows/hides it on dictation-state events.
fn create_overlay(app: &tauri::AppHandle) -> tauri::Result<()> {
    const W: f64 = 230.0;
    const H: f64 = 52.0;
    let builder = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html".into()))
        .title("suzune overlay")
        .inner_size(W, H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        // Kept visible always: a hidden WKWebView never loads, so its JS
        // could never receive the event that would show it. The pill is
        // fully transparent + click-through while idle instead.
        .accept_first_mouse(true);
    let window = builder.build()?;
    // Builder-time positioning is unreliable this early in setup (the
    // monitor may not be resolvable yet); place the built window instead.
    if let Ok(Some(monitor)) = window.current_monitor() {
        let size = monitor.size().to_logical::<f64>(monitor.scale_factor());
        let pos = tauri::LogicalPosition::new((size.width - W) / 2.0, size.height - H - 76.0);
        window.set_position(pos)?;
        log::info!("overlay placed at {:?} on {:?} monitor", pos, size);
    } else {
        log::warn!("overlay: no monitor resolved; leaving default position");
    }
    if std::env::var("SUZUNE_DEVTOOLS").is_ok() {
        window.open_devtools();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // Menubar-only app: no Dock icon, no app switcher entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let config_dir = app.path().app_config_dir()?;
            let mut settings = Settings::load(&config_dir);

            // Resolve the models directory: keep the user's configured
            // location if the models are actually there; otherwise point at
            // the app-data dir (self-contained, cross-platform) where the
            // first-run download will place them.
            if !models::models_present(&settings.models_root, &settings.cleanup_model) {
                if let Ok(default_root) = models::default_models_root(app.handle()) {
                    settings.models_root = default_root;
                }
            }
            settings.save(&config_dir)?; // materialize defaults for the user

            create_overlay(app.handle())?;

            let show_settings =
                MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit suzune", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_settings, &quit])?;
            TrayIconBuilder::with_id("suzune-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("suzune — hold your shortcut and speak")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // Closing the settings window hides it; the app lives in the tray.
            if let Some(main) = app.get_webview_window("main") {
                let main_handle = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_handle.hide();
                    }
                });
            }

            // Shared, editable settings for the shortcut handler and UI —
            // managed immediately so the settings/setup windows work even
            // before the coordinator starts.
            app.manage(SettingsState {
                settings: Mutex::new(settings.clone()),
                config_dir: config_dir.clone(),
                toggle_recording: Arc::new(AtomicBool::new(false)),
            });

            // Personalization's shared handle — managed here (not lazily on
            // first use) so it exists before `finish_startup` reaches for it
            // on both the synchronous and first-run-download startup paths.
            app.manage(HistoryState {
                buffer: Arc::new(Mutex::new(VecDeque::new())),
                next_id: Arc::new(AtomicU64::new(1)),
                config_dir: config_dir.clone(),
            });

            // Start the dictation engine now if the models are present;
            // otherwise download them first (first-run) and start after.
            if models::models_present(&settings.models_root, &settings.cleanup_model) {
                finish_startup(app.handle(), &settings)?;
            } else {
                log::info!("first run: models missing, starting download flow");
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                let handle = app.handle().clone();
                let settings_for_dl = settings.clone();
                std::thread::spawn(move || {
                    let root = settings_for_dl.models_root.clone();
                    match models::ensure_models(&handle, &root, &settings_for_dl.cleanup_model) {
                        Ok(()) => {
                            if let Err(e) = finish_startup(&handle, &settings_for_dl) {
                                models::emit_error(&handle, format!("startup failed: {e}"));
                            }
                        }
                        Err(e) => {
                            log::error!("model download failed: {e:#}");
                            models::emit_error(&handle, format!("{e}"));
                        }
                    }
                });
            }

            // SUZUNE_SELFTEST=record: drive one real record->cancel cycle
            // through the coordinator shortly after launch. Exercises the
            // overlay + mic path without a human at the keyboard (used for
            // automated visual verification; harmless to leave in).
            if std::env::var("SUZUNE_SELFTEST").as_deref() == Ok("record") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(25));
                    handle.state::<Coordinator>().send(Command::StartRecording);
                    std::thread::sleep(std::time::Duration::from_secs(6));
                    handle.state::<Coordinator>().send(Command::StopAndProcess);
                });
            }

            // SUZUNE_SELFTEST=demo: record a fixed 8s window starting 6s
            // after launch, for producing a demo capture with externally
            // synced audio playback.
            if std::env::var("SUZUNE_SELFTEST").as_deref() == Ok("demo") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(6));
                    handle.state::<Coordinator>().send(Command::StartRecording);
                    std::thread::sleep(std::time::Duration::from_secs(8));
                    handle.state::<Coordinator>().send(Command::StopAndProcess);
                });
            }

            // SUZUNE_SELFTEST=savetest: exercise the save_settings path
            // (persist + re-register shortcut + reload) without the UI, to
            // verify hotkey/mode changes apply live.
            if std::env::var("SUZUNE_SELFTEST").as_deref() == Ok("savetest") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let state = handle.state::<SettingsState>();
                    let mut new = state.settings.lock().unwrap().clone();
                    new.push_to_talk = false;
                    new.shortcut = "ctrl+alt+d".to_string();
                    new.injection_method = "ax".to_string();
                    let _ = new.save(&state.config_dir);
                    *state.settings.lock().unwrap() = new.clone();
                    match register_shortcut(&handle, &new.shortcut) {
                        Ok(()) => log::info!("savetest: re-registered {} ok", new.shortcut),
                        Err(e) => log::error!("savetest: re-register failed: {e}"),
                    }
                    handle
                        .state::<Coordinator>()
                        .send(Command::ReloadSettings(Box::new(new)));
                    log::info!("savetest: applied continuous mode + ctrl+alt+d + ax");
                });
            }

            // SUZUNE_SHOW_SETTINGS=1: open the settings window on launch
            // (for visual verification without clicking the tray).
            if std::env::var("SUZUNE_SHOW_SETTINGS").is_ok() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_input_devices,
            cancel_dictation,
            needs_setup,
            get_recent_history,
            submit_correction,
            list_corrections,
            clear_corrections
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
