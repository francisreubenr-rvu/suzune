mod coordinator;
mod settings;

use coordinator::{Command, Coordinator};
use settings::Settings;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[tauri::command]
fn get_settings(state: tauri::State<Settings>) -> Settings {
    state.inner().clone()
}

#[tauri::command]
fn cancel_dictation(coordinator: tauri::State<Coordinator>) {
    coordinator.send(Command::Cancel);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings = Settings::load(&config_dir);
            settings.save(&config_dir)?; // materialize defaults for the user

            let coordinator = Coordinator::start(app.handle().clone(), settings.clone());
            app.manage(coordinator);

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
