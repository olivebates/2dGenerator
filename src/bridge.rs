use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

pub struct Bridge {
    child: Child,
    tx: Sender<Value>,
    pub events: Receiver<Result<Value, String>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub model: String,
    pub display_name: String,
    pub default_reasoning_effort: String,
    pub supported_reasoning_efforts: Vec<ReasoningOption>,
    pub is_default: bool,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningOption {
    pub reasoning_effort: String,
}
impl Model {
    pub fn supports(&self, effort: &str) -> bool {
        self.supported_reasoning_efforts
            .iter()
            .any(|option| option.reasoning_effort == effort)
    }
}

pub fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("SEPIA_CODEX_PATH") {
        return path.into();
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let extensions = PathBuf::from(home).join(".vscode/extensions");
        if let Ok(entries) = std::fs::read_dir(extensions) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .starts_with("openai.chatgpt-")
                })
                .collect();
            paths.sort();
            for path in paths.into_iter().rev() {
                let exe = path.join("bin/windows-x86_64/codex.exe");
                if exe.is_file() {
                    return exe;
                }
            }
        }
    }
    "codex.exe".into()
}

impl Bridge {
    pub fn start(ctx: eframe::egui::Context) -> Result<Self, String> {
        let mut command = Command::new(executable());
        command
            .args([
                "app-server",
                "--stdio",
                "-c",
                "forced_login_method=\"chatgpt\"",
                "-c",
                "model_provider=\"openai\"",
            ])
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| {
            format!(
                "Could not start Codex: {e}. Install Codex or set SEPIA_CODEX_PATH to codex.exe."
            )
        })?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel::<Value>();
        let (event_tx, events) = mpsc::channel();
        let write_tx = event_tx.clone();
        let write_ctx = ctx.clone();
        thread::spawn(move || {
            for value in rx {
                if let Err(e) = writeln!(stdin, "{value}").and_then(|_| stdin.flush()) {
                    let _ = write_tx.send(Err(format!("Codex connection failed: {e}")));
                    write_ctx.request_repaint();
                    break;
                }
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if let Ok(value) = serde_json::from_str(&line) {
                            let _ = event_tx.send(Ok(value));
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(Err(e.to_string()));
                        break;
                    }
                }
                ctx.request_repaint();
            }
            let _ = event_tx.send(Err(
                "Codex disconnected. Restart the connection to continue.".into(),
            ));
            ctx.request_repaint();
        });
        Ok(Self { child, tx, events })
    }
    pub fn send(&self, value: Value) {
        let _ = self.tx.send(value);
    }
    pub fn request(&self, id: u64, method: &str, params: Value) {
        self.send(json!({"id":id,"method":method,"params":params}));
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires the installed Codex executable"]
    fn local_codex_handshake() {
        let bridge = Bridge::start(eframe::egui::Context::default()).unwrap();
        bridge.request(1,"initialize",json!({"clientInfo":{"name":"sepia_chat_test","version":"0.1.0"},"capabilities":{"experimentalApi":true}}));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let event = bridge
                .events
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .expect("Codex initialization timed out")
                .unwrap();
            if event["id"] == 1 {
                assert!(
                    event["error"].is_null(),
                    "Codex rejected initialization: {event}"
                );
                assert!(event["result"].is_object());
                break;
            }
        }
        bridge.send(json!({"method":"initialized","params":{}}));
        bridge.request(2, "model/list", json!({"includeHidden":false}));
        let models = loop {
            let event = bridge
                .events
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
                .unwrap();
            if event["id"] == 2 {
                assert!(event["error"].is_null(), "model/list failed: {event}");
                break serde_json::from_value::<Vec<Model>>(event["result"]["data"].clone())
                    .unwrap();
            }
        };
        assert!(!models.is_empty());
        bridge.request(3, "thread/start", json!({"cwd":std::env::current_dir().unwrap(),"ephemeral":true,"model":models[0].model,"sandbox":"workspace-write","approvalPolicy":"on-request","approvalsReviewer":"auto_review"}));
        loop {
            let event = bridge
                .events
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
                .unwrap();
            if event["id"] == 3 {
                assert!(
                    event["error"].is_null(),
                    "Auto-review thread setup failed: {event}"
                );
                assert_eq!(event["result"]["approvalsReviewer"], "auto_review");
                break;
            }
        }
    }
}
