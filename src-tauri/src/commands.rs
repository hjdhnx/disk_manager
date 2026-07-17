use crate::cleaner;
use crate::history::History;
use crate::models::{BigFilesResult, CleanResult, DiskInfo, DupResult, ScanOpts, SpaceSummary};
use chrono::Utc;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

/// 盘符列表（仅 Windows 实现；其它平台返回根分区）
#[cfg(windows)]
pub fn disks() -> Vec<DiskInfo> {
    use std::collections::HashSet;
    let mut set: HashSet<String> = HashSet::new();
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut result = Vec::new();
    for d in disks.list() {
        let mount = d.mount_point().to_string_lossy().to_string();
        if set.insert(mount.clone()) {
            let total = d.total_space();
            let available = d.available_space();
            let used = total.saturating_sub(available);
            let name = d.name().to_string_lossy().to_string();
            let letter = mount.clone();
            let is_system = mount.eq_ignore_ascii_case("C:\\")
                || mount.eq_ignore_ascii_case("/");
            result.push(DiskInfo {
                letter,
                name,
                mount,
                total,
                available,
                used,
                is_system,
            });
        }
    }
    // 排序：C: 在前
    result.sort_by(|a, b| b.is_system.cmp(&a.is_system).then(a.mount.cmp(&b.mount)));
    result
}

#[cfg(not(windows))]
pub fn disks() -> Vec<DiskInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .map(|d| {
            let mount = d.mount_point().to_string_lossy().to_string();
            let total = d.total_space();
            let available = d.available_space();
            DiskInfo {
                letter: mount.clone(),
                name: d.name().to_string_lossy().to_string(),
                mount,
                total,
                available,
                used: total.saturating_sub(available),
                is_system: mount == "/",
            }
        })
        .collect()
}

#[tauri::command]
pub fn list_disks() -> Vec<DiskInfo> {
    disks()
}

#[tauri::command]
pub async fn scan_space(
    app: AppHandle,
    root: String,
    opts: Option<ScanOpts>,
) -> Result<SpaceSummary, String> {
    let opts = opts.unwrap_or_default();
    let app2 = app.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        crate::scanner::scan_space(&app2, &root, &opts)
    })
    .await
    .map_err(|e| format!("扫描线程异常: {e}"))?;
    let _ = app.emit(
        "scan_done",
        crate::models::ScanProgress {
            stage: "done".to_string(),
            current: summary.root.clone(),
            scanned: summary.file_count,
            hashed: 0,
            bytes: summary.scanned_bytes,
            elapsed_ms: summary.duration_ms,
        },
    );
    Ok(summary)
}

#[tauri::command]
pub async fn scan_big_files(
    app: AppHandle,
    root: String,
    opts: Option<ScanOpts>,
) -> Result<BigFilesResult, String> {
    let opts = opts.unwrap_or_default();
    let app2 = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        crate::scanner::scan_big_files(&app2, &root, &opts)
    })
    .await
    .map_err(|e| format!("扫描线程异常: {e}"))?;
    let _ = app.emit(
        "scan_done",
        crate::models::ScanProgress {
            stage: "done".to_string(),
            current: r.root.clone(),
            scanned: r.items.len() as u64,
            hashed: 0,
            bytes: r.total_size,
            elapsed_ms: r.duration_ms,
        },
    );
    Ok(r)
}

#[tauri::command]
pub async fn find_duplicates(
    app: AppHandle,
    root: String,
    opts: Option<ScanOpts>,
) -> Result<DupResult, String> {
    let opts = opts.unwrap_or_default();
    let min_size = opts.min_size;
    let skip_system = opts.skip_system;
    let include_hidden = opts.include_hidden;
    let app2 = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        crate::hasher::find_duplicates(&app2, &root, min_size, skip_system, include_hidden)
    })
    .await
    .map_err(|e| format!("扫描线程异常: {e}"))?;
    let _ = app.emit(
        "scan_done",
        crate::models::ScanProgress {
            stage: "done".to_string(),
            current: r.root.clone(),
            scanned: r.scanned_files,
            hashed: r.hashed_files,
            bytes: r.total_waste,
            elapsed_ms: r.duration_ms,
        },
    );
    Ok(r)
}

/// 刷新已有重复组的状态：只检查文件是否还存在，不重新算 MD5
#[tauri::command]
pub async fn refresh_duplicates(
    app: AppHandle,
    result: DupResult,
) -> Result<DupResult, String> {
    let app2 = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        crate::refresh::refresh_duplicates(&app2, result)
    })
    .await
    .map_err(|e| format!("刷新线程异常: {e}"))?;
    Ok(r)
}

#[tauri::command]
pub fn trash_files(
    paths: Vec<String>,
    history: State<'_, Mutex<History>>,
) -> Result<CleanResult, String> {
    let r = cleaner::trash_files(paths);
    if !r.succeeded.is_empty() {
        let entry = crate::models::HistoryEntry {
            time: Utc::now().to_rfc3339(),
            action: "trash".to_string(),
            freed_bytes: r.freed_bytes,
            items: r.succeeded.clone(),
        };
        history.lock().unwrap().append(entry)?;
    }
    Ok(r)
}

#[tauri::command]
pub fn delete_files(
    paths: Vec<String>,
    history: State<'_, Mutex<History>>,
) -> Result<CleanResult, String> {
    let r = cleaner::delete_files(paths);
    if !r.succeeded.is_empty() {
        let entry = crate::models::HistoryEntry {
            time: Utc::now().to_rfc3339(),
            action: "delete".to_string(),
            freed_bytes: r.freed_bytes,
            items: r.succeeded.clone(),
        };
        history.lock().unwrap().append(entry)?;
    }
    Ok(r)
}

#[tauri::command]
pub fn open_in_explorer(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::path::Path;
        let p = Path::new(&path);
        use std::os::windows::process::CommandExt;
        let (exe, arg): (&str, String) = if p.exists() {
            ("explorer.exe", format!("/select,{}", path))
        } else {
            ("explorer.exe", path.clone())
        };
        std::process::Command::new(exe)
            .raw_arg(&arg)
            .spawn()
            .map_err(|e| format!("打开资源管理器失败: {e}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err("仅支持 Windows".into())
    }
}

#[tauri::command]
pub fn get_clean_history(history: State<'_, Mutex<History>>) -> Vec<crate::models::HistoryEntry> {
    history.lock().unwrap().all()
}

#[tauri::command]
pub fn clear_clean_history(history: State<'_, Mutex<History>>) -> Result<(), String> {
    history.lock().unwrap().clear()
}

/// 将导出内容写到应用数据目录下的 exports/<filename>，返回完整路径
/// 供前端的"导出列表"使用，写完后用 open_in_explorer 自动弹出定位
#[tauri::command]
pub fn save_export(
    app: AppHandle,
    filename: String,
    content: String,
) -> Result<String, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("获取数据目录失败: {e}"))?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).map_err(|e| format!("创建导出目录失败: {e}"))?;
    // 防止路径穿越：只保留文件名部分
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "export.md".to_string());
    let full = exports.join(&safe_name);
    std::fs::write(&full, content).map_err(|e| format!("写入文件失败: {e}"))?;
    Ok(full.to_string_lossy().to_string())
}

// raw_arg 来自 std::os::windows::process::CommandExt（Rust 1.62+ 稳定）