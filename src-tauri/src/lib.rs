mod coordinator;
mod settings;

use coordinator::{Command, Coordinator};
use settings::Settings;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[tauri::command]
fn get_settings(state: tauri::State<Settings>) -> Settings {
    state.inner().clone()
}

#[tauri::command]
fn cancel_dictation(coordinator: tauri::State<Coordinator>) {
    coordinator.send(Command::Cancel);
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
            let settings = Settings::load(&config_dir);
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

            let coordinator = Coordinator::start(app.handle().clone(), settings.clone());
            app.manage(coordinator);

            // WHISPR_SELFTEST=record: drive one real record->cancel cycle
            // through the coordinator shortly after launch. Exercises the
            // overlay + mic path without a human at the keyboard (used for
            // automated visual verification; harmless to leave in).
            if std::env::var("WHISPR_SELFTEST").as_deref() == Ok("record") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(25));
                    handle.state::<Coordinator>().send(Command::StartRecording);
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    handle.state::<Coordinator>().send(Command::Cancel);
                });
            }

            let push_to_talk = settings.push_to_talk;
            let shortcut: tauri_plugin_global_shortcut::Shortcut =
                settings.shortcut.parse().map_err(|e| {
                    anyhow::anyhow!("invalid shortcut '{}': {}", settings.shortcut, e)
                })?;
            app.global_shortcut().on_shortcut(shortcut, {
                // Toggle mode tracks recording locally; the coordinator
                // ignores redundant commands so drift is self-correcting.
                let toggle_recording = std::sync::atomic::AtomicBool::new(false);
                move |app, _shortcut, event| {
                    let coordinator = app.state::<Coordinator>();
                    if push_to_talk {
                        match event.state() {
                            ShortcutState::Pressed => coordinator.send(Command::StartRecording),
                            ShortcutState::Released => coordinator.send(Command::StopAndProcess),
                        }
                    } else if event.state() == ShortcutState::Pressed {
                        let now_recording = !toggle_recording
                            .fetch_xor(true, std::sync::atomic::Ordering::SeqCst);
                        coordinator.send(if now_recording {
                            Command::StartRecording
                        } else {
                            Command::StopAndProcess
                        });
                    }
                }
            })?;

            app.manage(settings);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_settings, cancel_dictation])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
