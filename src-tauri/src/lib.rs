mod coordinator;
mod models;
mod settings;

use coordinator::{Command, Coordinator};
use settings::Settings;
use std::sync::atomic::{AtomicBool, Ordering};
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
    whispr_audio::input_device_names()
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
    let coordinator = Coordinator::start(app.clone(), settings.clone());
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
        .title("whispr overlay")
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
    if std::env::var("WHISPR_DEVTOOLS").is_ok() {
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
            let quit = MenuItem::with_id(app, "quit", "Quit whispr", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_settings, &quit])?;
            TrayIconBuilder::with_id("whispr-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("whispr — hold your shortcut and speak")
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

            // WHISPR_SELFTEST=record: drive one real record->cancel cycle
            // through the coordinator shortly after launch. Exercises the
            // overlay + mic path without a human at the keyboard (used for
            // automated visual verification; harmless to leave in).
            if std::env::var("WHISPR_SELFTEST").as_deref() == Ok("record") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(25));
                    handle.state::<Coordinator>().send(Command::StartRecording);
                    std::thread::sleep(std::time::Duration::from_secs(6));
                    handle.state::<Coordinator>().send(Command::StopAndProcess);
                });
            }

            // WHISPR_SELFTEST=demo: record a fixed 8s window starting 6s
            // after launch, for producing a demo capture with externally
            // synced audio playback.
            if std::env::var("WHISPR_SELFTEST").as_deref() == Ok("demo") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(6));
                    handle.state::<Coordinator>().send(Command::StartRecording);
                    std::thread::sleep(std::time::Duration::from_secs(8));
                    handle.state::<Coordinator>().send(Command::StopAndProcess);
                });
            }

            // WHISPR_SELFTEST=savetest: exercise the save_settings path
            // (persist + re-register shortcut + reload) without the UI, to
            // verify hotkey/mode changes apply live.
            if std::env::var("WHISPR_SELFTEST").as_deref() == Ok("savetest") {
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

            // WHISPR_SHOW_SETTINGS=1: open the settings window on launch
            // (for visual verification without clicking the tray).
            if std::env::var("WHISPR_SHOW_SETTINGS").is_ok() {
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
            needs_setup
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
