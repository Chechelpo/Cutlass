use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Serialize, Deserialize};


#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
pub enum LogLevel {
    Trace = 10,
    Debug = 20,
    Info = 30,
    Warn = 40,
    Error = 50,
}

#[derive(Debug, Serialize)]
pub struct LogRecord {
    pub level: LogLevel,
    pub source: String,
    pub event: String,

    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, serde_json::Value>,
}


#[derive(Clone)]
pub struct Logger {
    source: String,
    level: LogLevel,
    output: Arc<Mutex<File>>,
}


impl Logger {
    pub fn new(
        source: impl Into<String>,
        directory: PathBuf,
        level: LogLevel,
    ) -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("latest.log"))
            .expect("Could not open log file");

        Logger {
            source: source.into(),
            level,
            output: Arc::new(Mutex::new(file)),
        }
    }


    fn write(
        &self,
        level: LogLevel,
        event: impl Into<String>,
        fields: HashMap<String, serde_json::Value>,
    ) {
        if level < self.level {
            return;
        }

        let record = LogRecord {
            level,
            source: self.source.clone(),
            event: event.into(),
            fields,
        };

        let serialized =
            serde_json::to_string(&record)
                .expect("Failed to serialize log");

        let mut file =
            self.output.lock().unwrap();

        writeln!(
            file,
            "{}",
            serialized
        )
            .expect("Failed writing log");
    }


    pub fn info(
        &self,
        event: impl Into<String>,
    ) {
        self.write(
            LogLevel::Info,
            event,
            HashMap::new(),
        );
    }


    pub fn error(
        &self,
        event: impl Into<String>,
    ) {
        self.write(
            LogLevel::Error,
            event,
            HashMap::new(),
        );
    }


    pub fn event(
        &self,
        level: LogLevel,
        event: impl Into<String>,
        fields: HashMap<String, serde_json::Value>,
    ) {
        self.write(
            level,
            event,
            fields,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_log_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir()
            .join(format!("logger_test_{}", id));

        fs::create_dir_all(&dir).unwrap();

        dir
    }

    #[test]
    fn logger_creates_log_file() {
        let dir = temp_log_dir();

        let _logger = Logger::new(
            "test",
            dir.clone(),
            LogLevel::Info,
        );

        assert!(
            dir.join("latest.log").exists()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn logger_writes_info_events() {
        let dir = temp_log_dir();

        let logger = Logger::new(
            "sandbox",
            dir.clone(),
            LogLevel::Info,
        );

        logger.info("sandbox started");

        let contents = fs::read_to_string(
            dir.join("latest.log")
        )
            .unwrap();

        assert!(contents.contains("sandbox started"));
        assert!(contents.contains("sandbox"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn logger_respects_log_level() {
        let dir = temp_log_dir();

        let logger = Logger::new(
            "sandbox",
            dir.clone(),
            LogLevel::Error,
        );

        logger.info("should not appear");
        logger.error("should appear");

        let contents = fs::read_to_string(
            dir.join("latest.log")
        )
            .unwrap();

        assert!(!contents.contains("should not appear"));
        assert!(contents.contains("should appear"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn logger_serializes_fields() {
        let dir = temp_log_dir();

        let logger = Logger::new(
            "tool",
            dir.clone(),
            LogLevel::Debug,
        );

        let mut fields = HashMap::new();

        fields.insert(
            "command".into(),
            serde_json::json!("ls"),
        );

        fields.insert(
            "exit_code".into(),
            serde_json::json!(0),
        );

        logger.event(
            LogLevel::Debug,
            "command executed",
            fields,
        );

        let contents = fs::read_to_string(
            dir.join("latest.log")
        )
            .unwrap();

        assert!(contents.contains("command executed"));
        assert!(contents.contains("ls"));
        assert!(contents.contains("0"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn logger_clones_share_output() {
        let dir = temp_log_dir();

        let logger = Logger::new(
            "shared",
            dir.clone(),
            LogLevel::Info,
        );

        let logger_clone = logger.clone();

        logger.info("first");
        logger_clone.info("second");

        let contents = fs::read_to_string(
            dir.join("latest.log")
        )
            .unwrap();

        assert!(contents.contains("first"));
        assert!(contents.contains("second"));

        fs::remove_dir_all(dir).unwrap();
    }
}