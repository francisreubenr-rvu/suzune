//! Single-threaded dictation state machine (Handy's coordinator pattern):
//! all pipeline state lives on one worker thread; the hotkey handler and UI
//! only send commands. This makes shortcut races (rapid press/release,
//! double-fire) harmless by construction.

use crate::personalization;
use crate::settings::Settings;
use crate::HistoryEntry;
use crate::HistoryState;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use tauri::{AppHandle, Emitter};
use suzune_asr::{Engine, EngineKind};
use suzune_audio::Recorder;
use suzune_cleanup::{CleanupClient, FewShotExample, GrammarLevel, LlamaServer, LlamaServerConfig, Tone};

#[derive(Debug)]
pub enum Command {
    StartRecording,
    StopAndProcess,
    Cancel,
    /// Apply edited settings without restarting the app. Injection method,
    /// input device, and cleanup on/off take effect immediately; changing
    /// the ASR engine or model still needs a restart (noted in the UI).
    /// Grammar-level/tone changes rebuild the cleanup client in place
    /// without respawning llama-server.
    ReloadSettings(Box<Settings>),
    /// A correction was just submitted (or the store was cleared) — re-read
    /// corrections.jsonl and rebuild the derived vocabulary from disk.
    ReloadCorrections,
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
    /// the cleanup server if enabled. `history` is the shared handle also
    /// managed as Tauri state, so the Settings window's history/corrections
    /// commands and this worker thread see the same data.
    pub fn start(app: AppHandle, settings: Settings, history: HistoryState) -> Coordinator {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("suzune-coordinator".into())
            .spawn(move || worker(app, settings, history, rx))
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
    history: HistoryState,
    corrections: Vec<personalization::CorrectionRecord>,
    vocab: HashMap<String, String>,
}

fn worker(app: AppHandle, settings: Settings, history: HistoryState, rx: Receiver<Command>) {
    let mut w = Worker::new(app, settings, history);
    w.emit(StateEvent::Idle);
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::StartRecording => w.start_recording(),
            Command::StopAndProcess => w.stop_and_process(),
            Command::Cancel => w.cancel(),
            Command::ReloadSettings(s) => w.reload_settings(*s),
            Command::ReloadCorrections => w.reload_corrections(),
        }
    }
}

impl Worker {
    fn new(app: AppHandle, settings: Settings, history: HistoryState) -> Worker {
        let corrections = personalization::load_corrections(&history.config_dir);
        let vocab = personalization::load_vocab_map(&history.config_dir);
        let mut w = Worker {
            app,
            settings,
            engine: None,
            _llama: None,
            cleanup: None,
            phase: Phase::Idle,
            history,
            corrections,
            vocab,
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
        let base_url = format!("http://127.0.0.1:{}", self.settings.cleanup_port);
        let level = GrammarLevel::from_setting(&self.settings.grammar_level);
        let tone = Tone::from_setting(&self.settings.tone);
        let probe = CleanupClient::new(&base_url, level, tone);
        if probe.is_healthy() {
            // A server from a previous run (or user-managed Ollama-style
            // setup) is already on the port — reuse it instead of failing.
            log::info!("reusing existing cleanup server on port {}", self.settings.cleanup_port);
            self.cleanup = Some(probe);
            return;
        }
        let gguf = self.settings.models_root.join(&self.settings.cleanup_model);
        let log_path = std::env::temp_dir().join("suzune-llama-server.log");
        match LlamaServer::spawn(LlamaServerConfig {
            server_binary_path: self.settings.llama_server_path.clone(),
            model_gguf_path: gguf,
            port: self.settings.cleanup_port,
            log_path,
        }) {
            Ok(server) => {
                self.cleanup = Some(CleanupClient::new(&base_url, level, tone));
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
        let mut recorder =
            Recorder::with_preferred_device(self.settings.input_device.clone());
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
        if audio.len() < suzune_audio::FRAME_SAMPLES * 10 {
            return Ok(None); // <300ms: accidental tap
        }
        let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
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
        let mut raw = transcript.text.trim().to_string();
        if raw.is_empty() {
            // Silence in, nothing out — tell the user instead of vanishing.
            // A near-zero peak means the mic heard nothing (wrong device,
            // muted, or a Continuity mic in another room).
            anyhow::bail!(
                "didn't catch any speech{}",
                if peak < 0.005 { " — check your microphone" } else { "" }
            );
        }

        // Personal vocabulary runs before cleanup (deterministic, no LLM)
        // so the grammar pass sees the corrected term in context. Opt-in
        // only — an empty vocab map (personalization off, or on but no
        // corrections mined into it yet) is a no-op.
        if self.settings.personalization_enabled && !self.vocab.is_empty() {
            raw = personalization::apply_vocabulary(&raw, &self.vocab);
        }

        let text = match &self.cleanup {
            Some(client) => {
                let selected = if self.settings.personalization_enabled {
                    personalization::select_few_shot(&raw, &self.corrections, 4)
                } else {
                    Vec::new()
                };
                let few_shot: Vec<FewShotExample> = selected
                    .iter()
                    .map(|c| FewShotExample {
                        input: &c.raw,
                        output: &c.corrected,
                    })
                    .collect();
                match client.clean(&raw, &few_shot) {
                    Ok(cleaned) if !cleaned.is_empty() => cleaned,
                    Ok(_) => raw.clone(),
                    Err(e) => {
                        log::warn!("cleanup failed, injecting raw transcript: {}", e);
                        raw.clone()
                    }
                }
            }
            None => raw.clone(),
        };
        // Log only lengths, never the content — this is a privacy tool and
        // the app log is not a place for the user's dictated words.
        log::info!("transcript: {} chars raw -> {} chars cleaned", raw.len(), text.len());

        if self.settings.personalization_enabled {
            self.push_history(raw.clone(), text.clone());
        }

        let primary = suzune_inject::InjectionMethod::from_setting(&self.settings.injection_method);
        let method = suzune_inject::inject_auto_with_primary(&text, primary)
            .map_err(|e| anyhow::anyhow!("injection: {}", e))?;
        Ok(Some((text, format!("{:?}", method))))
    }

    /// Append a rolling, in-memory-only (never persisted until the user
    /// actively corrects it) history entry, capped at 50, and notify the
    /// Settings window so it can refresh without polling.
    fn push_history(&mut self, raw: String, cleaned: String) {
        let id = self.history.next_id.fetch_add(1, Ordering::SeqCst);
        let ts = personalization::now_unix();
        {
            let mut buf = self.history.buffer.lock().unwrap();
            buf.push_back(HistoryEntry { id, raw, cleaned, ts });
            while buf.len() > 50 {
                buf.pop_front();
            }
        }
        if let Err(e) = self.app.emit("history-updated", ()) {
            log::error!("history-updated emit failed: {}", e);
        }
    }

    /// Re-read corrections.jsonl and rebuild the derived vocabulary — called
    /// after `submit_correction` or `clear_corrections` from the Settings
    /// window.
    fn reload_corrections(&mut self) {
        self.corrections = personalization::load_corrections(&self.history.config_dir);
        self.vocab = personalization::build_vocab_map(&self.corrections);
        if let Err(e) = personalization::save_vocab_map(&self.history.config_dir, &self.vocab) {
            log::warn!("could not persist derived vocabulary: {}", e);
        }
        log::info!(
            "corrections reloaded: {} stored, {} vocabulary entries",
            self.corrections.len(),
            self.vocab.len()
        );
    }

    fn cancel(&mut self) {
        if let Phase::Recording(mut r) = std::mem::replace(&mut self.phase, Phase::Idle) {
            let _ = r.stop();
        }
        self.emit(StateEvent::Idle);
    }

    /// Apply live-editable settings. The cleanup server is (re)started or
    /// torn down to match `cleanup_enabled`; injection method and input
    /// device are simply read from `self.settings` on the next dictation.
    /// A grammar-level/tone-only change rebuilds the cleanup client in
    /// place without respawning llama-server (same model, same port, just
    /// a different system prompt).
    fn reload_settings(&mut self, new: Settings) {
        let cleanup_was = self.settings.cleanup_enabled;
        let port_changed = new.cleanup_port != self.settings.cleanup_port;
        let model_changed = new.cleanup_model != self.settings.cleanup_model;
        let mode_changed =
            new.grammar_level != self.settings.grammar_level || new.tone != self.settings.tone;
        self.settings = new;

        if !self.settings.cleanup_enabled {
            // Turning cleanup off: drop the client and kill the server.
            self.cleanup = None;
            self._llama = None;
        } else if !cleanup_was || port_changed || model_changed || self.cleanup.is_none() {
            // Turning cleanup on, or its server config changed: restart it.
            self.cleanup = None;
            self._llama = None;
            self.start_cleanup_if_enabled();
        } else if mode_changed {
            let base_url = format!("http://127.0.0.1:{}", self.settings.cleanup_port);
            self.cleanup = Some(CleanupClient::new(
                &base_url,
                GrammarLevel::from_setting(&self.settings.grammar_level),
                Tone::from_setting(&self.settings.tone),
            ));
            log::info!(
                "cleanup client rebuilt in place (grammar={}, tone={})",
                self.settings.grammar_level, self.settings.tone
            );
        }
        log::info!("settings reloaded (injection={}, ptt={})",
            self.settings.injection_method, self.settings.push_to_talk);
    }
}
