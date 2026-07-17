use crate::models::HistoryEntry;
use std::path::PathBuf;
use std::sync::Mutex;

/// 清理历史记录：保存到 app_local_data_dir/clean_history.json
pub struct History {
    pub path: PathBuf,
    pub entries: Mutex<Vec<HistoryEntry>>,
}

impl History {
    pub fn load(path: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            entries: Mutex::new(entries),
        }
    }

    pub fn all(&self) -> Vec<HistoryEntry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut g = self.entries.lock().unwrap();
        g.clear();
        self.persist(&g)
    }

    pub fn append(&self, entry: HistoryEntry) -> Result<(), String> {
        let mut g = self.entries.lock().unwrap();
        // 只保留最近 500 条，避免无限增长
        let drop = g.len().saturating_sub(499);
        if drop > 0 {
            g.drain(0..drop);
        }
        g.push(entry);
        self.persist(&g)
    }

    fn persist(&self, g: &[HistoryEntry]) -> Result<(), String> {
        let json = serde_json::to_string_pretty(g).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, json).map_err(|e| e.to_string())
    }
}