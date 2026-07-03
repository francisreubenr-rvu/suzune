//! Manages an embedded `llama-server` child process serving the cleanup LLM.

use anyhow::{anyhow, bail, Context, Result};
use std::fs::File;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Bounded wait for the server to report healthy before giving up.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(90);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Configuration for spawning an embedded `llama-server` instance.
#[derive(Debug, Clone)]
pub struct LlamaServerConfig {
    /// Path to the `llama-server` binary (e.g. `/opt/homebrew/bin/llama-server`).
    pub server_binary_path: PathBuf,
    /// Path to the GGUF model file to load.
    pub model_gguf_path: PathBuf,
    /// Local port the server should listen on.
    pub port: u16,
    /// File to which the child's stdout/stderr are redirected.
    pub log_path: PathBuf,
}

/// A running `llama-server` child process. Killed automatically on drop.
pub struct LlamaServer {
    child: Child,
    port: u16,
}

impl LlamaServer {
    /// Spawn `llama-server` with the given config and block until it reports
    /// healthy (or the bounded retry window expires).
    pub fn spawn(config: LlamaServerConfig) -> Result<Self> {
        if !config.server_binary_path.exists() {
            bail!(
                "llama-server binary not found at {}",
                config.server_binary_path.display()
            );
        }
        if !config.model_gguf_path.exists() {
            bail!(
                "cleanup model GGUF not found at {}",
                config.model_gguf_path.display()
            );
        }
        ensure_port_available(config.port)?;

        if let Some(parent) = config.log_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating log directory {}", parent.display())
            })?;
        }
        let log_out = File::create(&config.log_path)
            .with_context(|| format!("creating log file {}", config.log_path.display()))?;
        let log_err = log_out
            .try_clone()
            .context("cloning log file handle for stderr")?;

        let child = Command::new(&config.server_binary_path)
            .arg("-m")
            .arg(&config.model_gguf_path)
            .arg("--port")
            .arg(config.port.to_string())
            .arg("-ngl")
            .arg("99")
            // Small context: system prompt + one utterance. Caps the KV
            // cache so the cleanup layer stays light in RAM.
            .arg("-c")
            .arg("2048")
            .stdout(Stdio::from(log_out))
            .stderr(Stdio::from(log_err))
            .spawn()
            .with_context(|| {
                format!(
                    "spawning llama-server ({})",
                    config.server_binary_path.display()
                )
            })?;

        let mut server = LlamaServer {
            child,
            port: config.port,
        };
        server.wait_for_health(&config.log_path)?;
        Ok(server)
    }

    /// The port this server instance is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    fn wait_for_health(&mut self, log_path: &Path) -> Result<()> {
        let deadline = Instant::now() + HEALTH_TIMEOUT;
        let url = format!("http://127.0.0.1:{}/health", self.port);
        loop {
            // If the child already exited, spawning is not going to succeed.
            if let Some(status) = self
                .child
                .try_wait()
                .context("polling llama-server child status")?
            {
                bail!(
                    "llama-server exited early ({status}) before becoming healthy; see log at {}",
                    log_path.display()
                );
            }

            if let Ok(resp) = ureq::get(&url).timeout(Duration::from_secs(2)).call() {
                if resp.status() == 200 {
                    return Ok(());
                }
            }

            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                bail!(
                    "llama-server did not become healthy within {:?}; see log at {}",
                    HEALTH_TIMEOUT,
                    log_path.display()
                );
            }
            std::thread::sleep(HEALTH_POLL_INTERVAL);
        }
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Check that nothing is already listening on `port`. Binding and immediately
/// releasing the socket is a best-effort check (race is possible against a
/// concurrent bind), but it gives a clear error in the common case where a
/// stale server is already running on this port.
fn ensure_port_available(port: u16) -> Result<()> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(e) => Err(anyhow!(
            "port {port} is already in use (is another llama-server or process bound to it?): {e}"
        )),
    }
}
