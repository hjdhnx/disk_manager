use crate::models::{DupGroup, DupResult, FileInfo};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

/// 刷新已有重复组的状态：仅检查文件是否存在，过滤掉无副本或全部已删的组，
/// 重新计算 keep_index / waste，不重新计算 MD5。
pub fn refresh_duplicates(app: &AppHandle, mut result: DupResult) -> DupResult {
    let start = Instant::now();
    for g in result.groups.iter_mut() {
        g.files.retain(|f| Path::new(&f.path).exists());
        if g.files.is_empty() {
            g.keep_index = 0;
            g.waste = 0;
            continue;
        }
        // 重新按 depth asc + modified desc 排序并选 keep
        g.files.sort_by(|a, b| a.depth.cmp(&b.depth).then(b.modified.cmp(&a.modified)));
        g.keep_index = 0;
        g.waste = (g.files.len() as u64).saturating_sub(1) * g.size;
    }
    result.groups.retain(|g| g.files.len() >= 2);
    result.groups.sort_by(|a, b| b.waste.cmp(&a.waste));
    result.total_waste = result.groups.iter().map(|g| g.waste).sum();
    // 修正统计：保持 hashed_files 不变，scanned_files 取剩余文件数
    result.scanned_files = result.groups.iter().map(|g| g.files.len() as u64).sum();
    result.duration_ms = start.elapsed().as_millis();
    let _ = app.emit(
        "scan_done",
        crate::models::ScanProgress {
            stage: "refreshed".to_string(),
            current: result.root.clone(),
            scanned: result.scanned_files,
            hashed: 0,
            bytes: result.total_waste,
            elapsed_ms: result.duration_ms,
        },
    );
    result
}