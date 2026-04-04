use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;
use tokio::task;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    reader: Option<Box<dyn Read + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    cols: u16,
    rows: u16,
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
            master: pair.master,
            writer: Arc::new(Mutex::new(writer)),
            reader: Some(reader),
            child: Some(child),
            cols,
            rows,
        })
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    pub async fn start_output_pump(&mut self, tx: mpsc::Sender<Vec<u8>>) -> Result<()> {
        let mut reader = self
            .reader
            .take()
            .context("PTY output pump already started")?;

        task::spawn_blocking(move || {
            let mut buffer = vec![0_u8; 8192];

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.blocking_send(buffer[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(())
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

    pub async fn write_all(&self, bytes: &[u8]) -> Result<()> {
        let writer = Arc::clone(&self.writer);
        let payload = bytes.to_vec();

        task::spawn_blocking(move || -> Result<()> {
            let mut guard = writer.lock().expect("PTY writer mutex poisoned");
            guard
                .write_all(&payload)
                .context("failed to write to PTY")?;
            guard.flush().context("failed to flush PTY writer")?;
            Ok(())
        })
        .await??;

        Ok(())
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;

        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to resize PTY")?;

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
