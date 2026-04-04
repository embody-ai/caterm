use tokio::sync::mpsc;

use crate::pty::PtySession;

use super::snapshot::PaneSnapshot;

pub struct Pane {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub size: u16,
    pub shell: String,
    pub pty: PtySession,
    pub output_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    pub output_history: String,
    pub pending_output: String,
    pub dropped_output_bytes: usize,
    pub exit_code: Option<u32>,
}

impl Pane {
    const MAX_OUTPUT_HISTORY_BYTES: usize = 8192;
    const MAX_PENDING_OUTPUT_BYTES: usize = 32768;
    const MAX_EMIT_OUTPUT_BYTES: usize = 4096;

    pub fn matches_target(&self, target: &str) -> bool {
        self.name == target || self.index.to_string() == target
    }

    pub fn snapshot(&self) -> PaneSnapshot {
        PaneSnapshot {
            id: self.id,
            index: self.index,
            name: self.name.clone(),
            size: self.size,
            shell: self.shell.clone(),
            exit_code: self.exit_code,
        }
    }

    pub fn push_output_history(&mut self, chunk: &str) {
        self.output_history.push_str(chunk);
        if self.output_history.len() > Self::MAX_OUTPUT_HISTORY_BYTES {
            let trim_to = self.output_history.len() - Self::MAX_OUTPUT_HISTORY_BYTES;
            self.output_history.drain(..trim_to);
        }
    }

    pub fn enqueue_output(&mut self, chunk: &str) {
        self.push_output_history(chunk);
        self.pending_output.push_str(chunk);
        if self.pending_output.len() > Self::MAX_PENDING_OUTPUT_BYTES {
            let trim_to = self.pending_output.len() - Self::MAX_PENDING_OUTPUT_BYTES;
            self.pending_output.drain(..trim_to);
            self.dropped_output_bytes += trim_to;
        }
    }

    pub fn take_output_chunk(&mut self) -> Option<String> {
        if self.pending_output.is_empty() && self.dropped_output_bytes == 0 {
            return None;
        }

        let mut output = String::new();
        if self.dropped_output_bytes > 0 {
            output.push_str(&format!(
                "[caterm dropped {} bytes of PTY output]\r\n",
                self.dropped_output_bytes
            ));
            self.dropped_output_bytes = 0;
        }

        let take_len = self
            .pending_output
            .char_indices()
            .take_while(|(idx, _)| *idx < Self::MAX_EMIT_OUTPUT_BYTES)
            .last()
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or_else(|| self.pending_output.len().min(Self::MAX_EMIT_OUTPUT_BYTES));

        if take_len > 0 {
            output.push_str(&self.pending_output[..take_len]);
            self.pending_output.drain(..take_len);
        }

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Pane;
    use tokio::sync::mpsc;

    #[test]
    fn output_buffer_drops_oldest_bytes_and_reports_it() {
        let mut pane = Pane {
            id: 1,
            index: 0,
            name: "pane-1".to_string(),
            size: 100,
            shell: "sh".to_string(),
            pty: crate::pty::PtySession::spawn("sh", 80, 24).expect("spawn pty"),
            output_rx: mpsc::unbounded_channel().1,
            output_history: String::new(),
            pending_output: String::new(),
            dropped_output_bytes: 0,
            exit_code: None,
        };

        pane.enqueue_output(&"a".repeat(40000));
        let output = pane.take_output_chunk().expect("buffered output");

        assert!(output.contains("[caterm dropped"));
        assert!(output.len() <= 4096 + 64);
    }
}
