use std::env;
use std::path::Path;

use anyhow::Result;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::protocol::{AgentEvent, ClientMessage};
use crate::pty::PtySession;

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 32;

pub struct AgentConfig {
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
}

impl AgentConfig {
    pub fn from_env() -> Self {
        Self {
            shell: resolve_shell(),
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        }
    }
}

pub struct Agent {
    config: AgentConfig,
}

impl Agent {
    pub fn new(config: AgentConfig) -> Result<Self> {
        Ok(Self { config })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut pty = PtySession::spawn(&self.config.shell, self.config.cols, self.config.rows)?;
        let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(128);
        let (input_tx, mut input_rx) = mpsc::channel::<ClientMessage>(64);
        let (cols, rows) = pty.size();

        pty.start_output_pump(output_tx).await?;
        emit_event(&AgentEvent::AgentStarted {
            shell: self.config.shell.clone(),
            cols,
            rows,
        })
        .await?;

        let stdin_task = tokio::spawn(read_client_messages(input_tx));

        loop {
            tokio::select! {
                maybe_chunk = output_rx.recv() => {
                    match maybe_chunk {
                        Some(chunk) => {
                            let data = String::from_utf8_lossy(&chunk).into_owned();
                            emit_event(&AgentEvent::PtyOutput { data }).await?;
                        }
                        None => break,
                    }
                }
                maybe_message = input_rx.recv() => {
                    match maybe_message {
                        Some(ClientMessage::PtyInput { data }) => {
                            pty.write_all(data.as_bytes()).await?;
                        }
                        Some(ClientMessage::Resize { cols, rows }) => {
                            pty.resize(cols, rows).await?;
                        }
                        Some(ClientMessage::Shutdown) | None => {
                            info!("shutting down agent loop");
                            pty.terminate().await?;
                            break;
                        }
                    }
                }
            }
        }

        finish_stdin_task(stdin_task).await?;

        let exit_code = pty.wait().await?;
        emit_event(&AgentEvent::PtyExited { exit_code }).await?;

        Ok(())
    }
}

async fn emit_event(event: &AgentEvent) -> Result<()> {
    let payload = serde_json::to_string(event)?;
    let mut stdout = io::stdout();

    stdout.write_all(payload.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;

    Ok(())
}

async fn read_client_messages(tx: mpsc::Sender<ClientMessage>) -> Result<()> {
    let stdin = io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<ClientMessage>(&line) {
            Ok(message) => {
                if tx.send(message).await.is_err() {
                    break;
                }
            }
            Err(error) => {
                error!(%error, raw = %line, "dropping invalid client message");
            }
        }
    }

    Ok(())
}

async fn finish_stdin_task(stdin_task: JoinHandle<Result<()>>) -> Result<()> {
    match stdin_task.await {
        Ok(result) => result,
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn resolve_shell() -> String {
    if let Ok(shell) = env::var("CATERM_SHELL") {
        if !shell.trim().is_empty() {
            return shell;
        }
    }

    if let Ok(shell) = env::var("SHELL") {
        if Path::new(&shell).exists() {
            return shell;
        }
    }

    "/bin/bash".to_string()
}
