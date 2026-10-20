use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Log JSON por líneas. Cada error lleva nivel, módulo, causa y acción.
#[derive(Clone)]
pub struct Logger {
    inner: Arc<Mutex<State>>,
}

struct State {
    file: File,
    path: PathBuf,
    max_bytes: u64,
    keep: u32,
}

impl Logger {
    pub fn open(dir: &Path, max_bytes: u64, keep: u32) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("hostig.log");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(State {
                file,
                path,
                max_bytes,
                keep,
            })),
        })
    }

    pub fn info(&self, module: &str, message: &str) {
        self.write("info", module, message, None, None);
    }

    pub fn error(&self, module: &str, message: &str, cause: &str, action: &str) {
        self.write("error", module, message, Some(cause), Some(action));
    }

    fn write(
        &self,
        level: &str,
        module: &str,
        message: &str,
        cause: Option<&str>,
        action: Option<&str>,
    ) {
        let ts = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let line = serde_json::json!({
            "ts": ts,
            "level": level,
            "module": module,
            "message": message,
            "cause": cause,
            "action": action,
        });
        let text = format!("{line}\n");
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if state.rotate_if_needed().is_err() {
            return;
        }
        let _ = state.file.write_all(text.as_bytes());
        let _ = state.file.flush();
    }
}

impl State {
    fn rotate_if_needed(&mut self) -> std::io::Result<()> {
        let len = self.file.metadata()?.len();
        if len < self.max_bytes {
            return Ok(());
        }
        let hold = self.path.with_file_name("hostig.log.hold");
        let placeholder = std::fs::File::create(&hold)?;
        drop(std::mem::replace(&mut self.file, placeholder));
        let oldest = self
            .path
            .with_file_name(format!("hostig.log.{}", self.keep));
        let _ = std::fs::remove_file(&oldest);
        for index in (1..self.keep).rev() {
            let from = self.path.with_file_name(format!("hostig.log.{index}"));
            let to = self
                .path
                .with_file_name(format!("hostig.log.{}", index + 1));
            let _ = std::fs::rename(&from, &to);
        }
        let rotated = self.path.with_file_name("hostig.log.1");
        std::fs::rename(&self.path, &rotated)?;
        self.file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let _ = std::fs::remove_file(&hold);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_error_queda_en_json_y_rota_por_tamano() {
        let dir = tempfile::tempdir().unwrap();
        let logger = Logger::open(dir.path(), 120, 2).unwrap();
        logger.error("ipc", "uno", "causa-uno", "accion-uno");
        logger.error("ipc", "dos", "causa-dos", "accion-dos");
        logger.error("ipc", "tres", "causa-tres", "accion-tres");
        let active = std::fs::read_to_string(dir.path().join("hostig.log")).unwrap();
        let mut combined = active.clone();
        for name in ["hostig.log.1", "hostig.log.2"] {
            if let Ok(text) = std::fs::read_to_string(dir.path().join(name)) {
                combined.push_str(&text);
            }
        }
        assert!(combined.contains("\"cause\":\"causa-uno\""), "{combined}");
        assert!(combined.contains("\"cause\":\"causa-tres\""), "{combined}");
        let last = active.lines().last().unwrap();
        let value: serde_json::Value = serde_json::from_str(last).unwrap();
        assert_eq!(value["level"], "error");
        assert_eq!(value["module"], "ipc");
        assert!(value["cause"].as_str().unwrap().starts_with("causa-"));
        assert!(value["action"].as_str().unwrap().starts_with("accion-"));
        assert!(dir.path().join("hostig.log.1").exists());
    }
}
