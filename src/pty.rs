use std::io::{Read, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};
use tokio::task;

pub struct PtySession {
    reader: Option<Box<dyn Read + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    _writer: Arc<Box<dyn Write + Send>>,
}

impl PtySession {
    pub fn spawn(shell: &str, cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to create PTY")?;

        let cmd = CommandBuilder::new(shell);
        let child = pair
            .slave
            .spawn_command(cmd)
            .context("failed to spawn shell")?;
        let reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone PTY reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take PTY writer")?;

        Ok(Self {
            _writer: Arc::new(writer),
            reader: Some(reader),
            child: Some(child),
        })
    }

    pub async fn start_discard_output(&mut self) -> Result<()> {
        let mut reader = self
            .reader
            .take()
            .context("PTY output pump already started")?;

        task::spawn_blocking(move || {
            let mut buffer = vec![0_u8; 8192];

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });

        Ok(())
    }

    pub async fn wait(&mut self) -> Result<u32> {
        let mut child = self.child.take().context("PTY child already awaited")?;
        let status = task::spawn_blocking(move || child.wait())
            .await?
            .context("failed to wait for shell")?;

        Ok(status.exit_code())
    }

    pub async fn terminate(&mut self) -> Result<()> {
        let child = self.child.as_mut().context("PTY child not available")?;
        child.kill().context("failed to terminate shell")?;
        Ok(())
    }
}
