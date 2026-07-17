use crate::models::{CleanItem, CleanResult};
use std::path::PathBuf;

/// 删除文件到回收站（可恢复），使用 trash crate
pub fn trash_files(paths: Vec<String>) -> CleanResult {
    let mut succeeded: Vec<CleanItem> = Vec::new();
    let mut failed: Vec<CleanItem> = Vec::new();
    let mut freed_bytes = 0u64;
    for p in &paths {
        let pb = PathBuf::from(p);
        let size = std::fs::metadata(&pb).map(|m| m.len()).unwrap_or(0);
        match trash::delete(&pb) {
            Ok(_) => {
                freed_bytes += size;
                succeeded.push(CleanItem {
                    path: p.clone(),
                    size,
                });
            }
            Err(_) => failed.push(CleanItem {
                path: p.clone(),
                size,
            }),
        }
    }
    CleanResult {
        succeeded,
        failed,
        freed_bytes,
    }
}

/// 永久删除（不可恢复）
pub fn delete_files(paths: Vec<String>) -> CleanResult {
    let mut succeeded: Vec<CleanItem> = Vec::new();
    let mut failed: Vec<CleanItem> = Vec::new();
    let mut freed_bytes = 0u64;
    for p in &paths {
        let pb = PathBuf::from(p);
        let size = std::fs::metadata(&pb).map(|m| m.len()).unwrap_or(0);
        let r = if pb.is_dir() {
            std::fs::remove_dir_all(&pb)
        } else {
            std::fs::remove_file(&pb)
        };
        if r.is_ok() {
            freed_bytes += size;
            succeeded.push(CleanItem {
                path: p.clone(),
                size,
            });
        } else {
            failed.push(CleanItem {
                path: p.clone(),
                size,
            });
        }
    }
    CleanResult {
        succeeded,
        failed,
        freed_bytes,
    }
}