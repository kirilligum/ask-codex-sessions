use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct DebugEvents {
    enabled: bool,
    lines: Arc<Mutex<Vec<String>>>,
}

impl DebugEvents {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn enabled() -> Self {
        Self {
            enabled: true,
            lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn log(&self, message: impl Into<String>) {
        if !self.enabled {
            return;
        }

        let message = message.into();
        eprintln!("[debug] {message}");
        if let Ok(mut lines) = self.lines.lock() {
            lines.push(message);
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().map(|lines| lines.clone()).unwrap_or_default()
    }
}
