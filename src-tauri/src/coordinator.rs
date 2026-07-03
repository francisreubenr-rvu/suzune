//! Single-threaded dictation state machine (Handy's coordinator pattern):
//! all pipeline state lives on one worker thread; the hotkey handler and UI
//! only send commands. This makes shortcut races (rapid press/release,
//! double-fire) harmless by construction.

use crate::settings::Settings;
use anyhow::Result;
use serde::Serialize;
use std::sync::mpsc::{Receiver, Sender};
use tauri::{AppHandle, Emitter};
use whispr_asr::{Engine, EngineKind};
use whispr_audio::Recorder;
use whispr_cleanup::{CleanupClient, LlamaServer, LlamaServerConfig};

#[derive(Debug)]
pub enum Command {
    StartRecording,
    StopAndProcess,
    Cancel,
}

/// Event payload emitted to the webview as "dictation-state".
#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StateEvent {
    Idle,
    Recording,
    Processing,
    Injected { text: String, method: String },
    Error { message: String },
}

pub struct Coordinator {
    tx: Sender<Command>,
}

impl Coordinator {
    /// Spawns the worker thread. Loads the ASR engine eagerly (S1: model
    /// stays resident — reload is seconds, dictation must not stall) and
    /// the cleanup server if enabled.
    pub fn start(app: AppHandle, settings: Settings) -> Coordinator {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("whispr-coordinator".into())
            .spawn(move || worker(app, settings, rx))
            .expect("spawn coordinator thread");
        Coordinator { tx }
    }

    pub fn send(&self, cmd: Command) {
        // A dead worker is unrecoverable app state; log rather than panic.
        if let Err(e) = self.tx.send(cmd) {
            log::error!("coordinator worker gone: {}", e);
        }
    }
}

enum Phase {
    Idle,
    Recording(Recorder),
}

struct Worker {
    app: AppHandle,
    settings: Settings,
    engine: Option<Engine>,
    // Held for its Drop (kills the llama-server child on app exit).
    _llama: Option<LlamaServer>,
    cleanup: Option<CleanupClient>,
    phase: Phase,
}

fn worker(app: AppHandle, settings: Settings, rx: Receiver<Command>) {
    let mut w = Worker::new(app, settings);
    w.emit(StateEvent::Idle);
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::StartRecording => w.start_recording(),
            Command::StopAndProcess => w.stop_and_process(),
            Command::Cancel => w.cancel(),
        }
    }
}

impl Worker {
    fn new(app: AppHandle, settings: Settings) -> Worker {
        let mut w = Worker {
            app,
            settings,
            engine: None,
            _llama: None,
            cleanup: None,
            phase: Phase::Idle,
        };
        if let Err(e) = w.load_engine() {
            w.emit(StateEvent::Error {
                message: format!("ASR engine failed to load: {}", e),
            });
        }
        w.start_cleanup_if_enabled();
        w
    }

    fn load_engine(&mut self) -> Result<()> {
        let t = std::time::Instant::now();
        let engine = Engine::load(EngineKind::ParakeetV2, &self.settings.models_root)?;
        log::info!("ASR engine loaded in {:.1}s", t.elapsed().as_secs_f32());
        self.engine = Some(engine);
        Ok(())
    }

    fn start_cleanup_if_enabled(&mut self) {
        if !self.settings.cleanup_enabled {
            return;
        }
        let gguf = self
            .settings
            .models_root
            .join("Qwen3-4B-Instruct-2507-Q4_K_M.gguf");
        let log_path = std::env::temp_dir().join("whispr-llama-server.log");
        match LlamaServer::spawn(LlamaServerConfig {
            server_binary_path: self.settings.llama_server_path.clone(),
            model_gguf_path: gguf,
            port: self.settings.cleanup_port,
            log_path,
        }) {
            Ok(server) => {
                self.cleanup = Some(CleanupClient::new(&format!(
                    "http://127.0.0.1:{}",
                    self.settings.cleanup_port
                )));
                self._llama = Some(server);
                log::info!("cleanup server up on port {}", self.settings.cleanup_port);
            }
            Err(e) => {
                // Cleanup is an enhancement; dictation must work without it.
                log::warn!("cleanup disabled: {}", e);
            }
        }
    }

    fn emit(&self, ev: StateEvent) {
        if let Err(e) = self.app.emit("dictation-state", &ev) {
            log::error!("emit failed: {}", e);
        }
    }

    fn start_recording(&mut self) {
        if matches!(self.phase, Phase::Recording(_)) {
            return; // key auto-repeat / double press
        }
        let mut recorder = Recorder::new();
        match recorder.start() {
            Ok(()) => {
                self.phase = Phase::Recording(recorder);
                self.emit(StateEvent::Recording);
            }
            Err(e) => self.emit(StateEvent::Error {
                message: format!("microphone: {}", e),
            }),
        }
    }

    fn stop_and_process(&mut self) {
        let recorder = match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Recording(r) => r,
            Phase::Idle => return,
        };
        self.emit(StateEvent::Processing);
        match self.process(recorder) {
            Ok(Some((text, method))) => self.emit(StateEvent::Injected { text, method }),
            Ok(None) => self.emit(StateEvent::Idle),
            Err(e) => self.emit(StateEvent::Error {
                message: e.to_string(),
            }),
        }
    }

    /// Push-to-talk/toggle both have an explicit stop, so the whole buffer
    /// is transcribed — no VAD endpointing here (M1 finding: endpointing is
    /// for a future hands-free mode).
    fn process(&mut self, mut recorder: Recorder) -> Result<Option<(String, String)>> {
        let audio = recorder.stop()?;
        if audio.len() < whispr_audio::FRAME_SAMPLES * 10 {
            return Ok(None); // <300ms: accidental tap
        }
        let engine = self
            .engine
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("ASR engine not loaded"))?;
        let transcript = match engine.transcribe(&audio, None) {
            Ok(t) => t,
            Err(e) => {
                // catch_unwind contract: a panicking engine must be dropped.
                self.engine = None;
                return Err(e);
            }
        };
        let raw = transcript.text.trim().to_string();
        if raw.is_empty() {
            return Ok(None);
        }
        let text = match &self.cleanup {
            Some(client) => match client.clean(&raw) {
                Ok(cleaned) if !cleaned.is_empty() => cleaned,
                Ok(_) => raw.clone(),
                Err(e) => {
                    log::warn!("cleanup failed, injecting raw transcript: {}", e);
                    raw.clone()
                }
            },
            None => raw.clone(),
        };
        let method = whispr_inject::inject_auto(&text)
            .map_err(|e| anyhow::anyhow!("injection: {}", e))?;
        Ok(Some((text, format!("{:?}", method))))
    }

    fn cancel(&mut self) {
        if let Phase::Recording(mut r) = std::mem::replace(&mut self.phase, Phase::Idle) {
            let _ = r.stop();
        }
        self.emit(StateEvent::Idle);
    }
}
