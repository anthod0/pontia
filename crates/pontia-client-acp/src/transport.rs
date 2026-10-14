use std::{
    collections::HashSet,
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use pontia_core::{Error, Result};
use serde_json::Value;
use tokio::{io::AsyncWrite, process::ChildStdin, sync::watch};

// SDK send_request/send_notification enqueue output. A control receipt must instead
// wait until the corresponding frame has actually been flushed to the peer pipe.
#[derive(Default)]
struct Written {
    keys: HashSet<String>,
    closed: bool,
}

pub(crate) struct Receipts {
    state: Mutex<Written>,
    changed: watch::Sender<u64>,
}

impl Receipts {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(Written::default()),
            changed: watch::channel(0).0,
        })
    }

    pub async fn wait(&self, key: String) -> Result<()> {
        let mut changes = self.changed.subscribe();
        loop {
            {
                let mut state = self.state.lock().unwrap();
                if state.keys.remove(&key) {
                    return Ok(());
                }
                if state.closed {
                    return Err(Error::ControlUnknown(
                        "ACP delivery could not be confirmed".into(),
                    ));
                }
            }
            changes
                .changed()
                .await
                .map_err(|_| Error::ControlUnknown("ACP connection closed".into()))?;
        }
    }

    pub fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.changed.send_modify(|version| *version += 1);
    }
}

pub(crate) struct FlushedWriter {
    stdin: ChildStdin,
    bytes: Vec<u8>,
    receipts: Arc<Receipts>,
}

impl FlushedWriter {
    pub fn new(stdin: ChildStdin, receipts: Arc<Receipts>) -> Self {
        Self {
            stdin,
            bytes: Vec::new(),
            receipts,
        }
    }

    fn flushed(&mut self) {
        while let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            if let Ok(frame) = serde_json::from_slice::<Value>(&self.bytes[..end]) {
                let key =
                    if let Some(id) = frame.get("id").filter(|_| frame.get("method").is_some()) {
                        Some(id.to_string())
                    } else if frame["method"] == "session/cancel" {
                        Some("session/cancel".into())
                    } else {
                        None
                    };
                if let Some(key) = key {
                    self.receipts.state.lock().unwrap().keys.insert(key);
                    self.receipts.changed.send_modify(|version| *version += 1);
                }
            }
            self.bytes.drain(..=end);
        }
    }
}

impl AsyncWrite for FlushedWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.stdin).poll_write(cx, bytes);
        match &result {
            Poll::Ready(Ok(count)) => self.bytes.extend_from_slice(&bytes[..*count]),
            Poll::Ready(Err(_)) => self.receipts.close(),
            _ => {}
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.stdin).poll_flush(cx);
        match &result {
            Poll::Ready(Ok(())) => self.flushed(),
            Poll::Ready(Err(_)) => self.receipts.close(),
            _ => {}
        }
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}
