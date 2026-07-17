use crate::models::{DirEntry, FileInfo, ScanOpts, SpaceSummary, BigFilesResult};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime};
use tauri::{AppHandle, Emitter};

/// Windows 系统目录名（大小写不敏感）集合，用于标记与跳过
const SYSTEM_NAMES: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "$winreagent",
    "$windows.~bt",
    "$windows.~ws",
    "recovery",
    "config.msi",
];

/// 判断目录名是否为系统目录
fn is_system_name(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    SYSTEM_NAMES.iter().any(|s| **s == n) || n.starts_with("$win")
}

#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x2 != 0 // */
}
#[cfg(not(windows))]
fn is_hidden(_meta: &std::fs::Metadata) -> bool {
    false
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 判断某个路径是否应跳过（系统/隐藏）
#[allow(dead_code)]
fn should_skip(p: &Path, meta: &std::fs::Metadata, opts: &ScanOpts) -> bool {
    let name = file_name(p);
    if !opts.include_hidden && is_hidden(meta) {
        return true;
    }
    if opts.skip_system && is_system_name(&name) {
        return true;
    }
    false
}

fn fmt_modified(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| format!("{}", chrono::DateTime::from_timestamp(d.as_secs() as i64, 0).unwrap_or_default().to_rfc3339()))
        .unwrap_or_default()
}

/// 进度发射器：累计计数 + 节流
struct Progress<'a> {
    app: &'a AppHandle,
    stage: &'a str,
    scanned: &'a AtomicU64,
    bytes: &'a AtomicU64,
    errors: &'a AtomicU64,
    start: Instant,
    last_emit: std::sync::Mutex<Instant>,
}

impl<'a> Progress<'a> {
    fn tick(&self, current: &str) {
        let now = Instant::now();
        let mut last = self.last_emit.lock().unwrap();
        if now.duration_since(*last).as_millis() < 120 {
            return;
        }
        *last = now;
        let _ = self.app.emit(
            "scan_progress",
            crate::models::ScanProgress {
                stage: self.stage.to_string(),
                current: current.to_string(),
                scanned: self.scanned.load(Ordering::Relaxed),
                hashed: 0,
                bytes: self.bytes.load(Ordering::Relaxed),
                elapsed_ms: self.start.elapsed().as_millis(),
            },
        );
    }
}

/// 聚合单个子目录：递归求和其下所有文件大小、文件数、目录数
fn aggregate_subdir(
    app: &AppHandle,
    dir: &Path,
    opts: &ScanOpts,
    scanned: &AtomicU64,
    bytes: &AtomicU64,
    errors: &AtomicU64,
    start: Instant,
) -> DirEntry {
    let name = file_name(dir);
    let is_system = is_system_name(&name);
    let mut size = 0u64;
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut big_files: Vec<FileInfo> = Vec::new();

    let progress = Progress {
        app,
        stage: "scanning",
        scanned,
        bytes,
        errors,
        start,
        last_emit: std::sync::Mutex::new(Instant::now()),
    };

    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // 入口过滤（仍会进入子项；仅做轻量过滤）
            let p = e.path();
            let nm = file_name(p);
            if !opts.include_hidden && p != dir {
                if let Ok(m) = p.metadata() {
                    if is_hidden(&m) {
                        return false;
                    }
                }
            }
            if opts.skip_system && is_system_name(&nm) && p != dir {
                return false;
            }
            true
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        let p = entry.path();
        let ft = entry.file_type();
        if ft.is_dir() {
            if p != dir {
                dir_count += 1;
            }
        } else if ft.is_file() {
            if let Ok(meta) = entry.metadata() {
                let s = meta.len();
                size += s;
                file_count += 1;
                scanned.fetch_add(1, Ordering::Relaxed);
                bytes.fetch_add(s, Ordering::Relaxed);
                if opts.with_big_files && s >= opts.min_size && big_files.len() < 50 {
                    big_files.push(FileInfo {
                        path: p.to_string_lossy().to_string(),
                        size: s,
                        modified: fmt_modified(&meta),
                        depth: entry.depth(),
                        is_dir: false,
                        is_system,
                    });
                }
            }
        }
        progress.tick(&p.to_string_lossy());
    }

    DirEntry {
        path: dir.to_string_lossy().to_string(),
        name,
        size,
        file_count,
        dir_count,
        is_system,
        big_files,
    }
}

/// 扫描空间占用：聚合 root 下的直接子目录大小 + root 本层文件
pub fn scan_space(app: &AppHandle, root: &str, opts: &ScanOpts) -> SpaceSummary {
    let start = Instant::now();
    let root_path = Path::new(root);
    let scanned = AtomicU64::new(0);
    let bytes = AtomicU64::new(0);
    let errors = AtomicU64::new(0);

    let mut subdir_paths: Vec<PathBuf> = Vec::new();
    let mut root_file_count = 0u64;
    let mut root_files: Vec<FileInfo> = Vec::new();

    if let Ok(rd) = std::fs::read_dir(root_path) {
        for e in rd.flatten() {
            let p = e.path();
            let nm = file_name(&p);
            if !opts.include_hidden {
                if let Ok(m) = e.metadata() {
                    if is_hidden(&m) {
                        continue;
                    }
                }
            }
            if opts.skip_system && is_system_name(&nm) && p != root_path {
                continue;
            }
            let ft = match e.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                subdir_paths.push(p);
            } else if ft.is_file() {
                if let Ok(meta) = e.metadata() {
                    let s = meta.len();
                    root_file_count += 1;
                    bytes.fetch_add(s, Ordering::Relaxed);
                    scanned.fetch_add(1, Ordering::Relaxed);
                    // 收集本层大文件，便于下钻时显示当前层级的文件占用
                    if opts.with_big_files && s >= opts.min_size && root_files.len() < 200 {
                        root_files.push(FileInfo {
                            path: p.to_string_lossy().to_string(),
                            size: s,
                            modified: fmt_modified(&meta),
                            depth: 0,
                            is_dir: false,
                            is_system: is_system_name(&nm),
                        });
                    }
                }
            }
        }
    }

    // 并行聚合各子目录
    let mut dirs: Vec<DirEntry> = subdir_paths
        .par_iter()
        .map(|d| aggregate_subdir(app, d, opts, &scanned, &bytes, &errors, start))
        .collect();

    dirs.sort_by(|a, b| b.size.cmp(&a.size));
    if dirs.len() > opts.top {
        dirs.truncate(opts.top);
    }

    SpaceSummary {
        root: root.to_string(),
        total_bytes: 0,
        used_bytes: 0,
        scanned_bytes: bytes.load(Ordering::Relaxed),
        file_count: root_file_count + dirs.iter().map(|d| d.file_count).sum::<u64>(),
        dir_count: dirs.len() as u64 + dirs.iter().map(|d| d.dir_count).sum::<u64>(),
        duration_ms: start.elapsed().as_millis(),
        skipped_errors: errors.load(Ordering::Relaxed),
        dirs,
        root_files,
    }
}

/// 扫描大文件：遍历 root，收集 size >= min_size 的文件，按 size 降序截前 5000
pub fn scan_big_files(app: &AppHandle, root: &str, opts: &ScanOpts) -> BigFilesResult {
    let start = Instant::now();
    let root_path = Path::new(root);
    let scanned = AtomicU64::new(0);
    let bytes = AtomicU64::new(0);
    let errors = AtomicU64::new(0);

    // 任务切分：root 直接子目录 + root 本层文件
    let mut subdir_paths: Vec<PathBuf> = Vec::new();
    let mut seed: Vec<FileInfo> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root_path) {
        for e in rd.flatten() {
            let p = e.path();
            let nm = file_name(&p);
            if !opts.include_hidden {
                if let Ok(m) = e.metadata() {
                    if is_hidden(&m) {
                        continue;
                    }
                }
            }
            if opts.skip_system && is_system_name(&nm) && p != root_path {
                continue;
            }
            let ft = match e.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                subdir_paths.push(p);
            } else if ft.is_file() {
                if let Ok(meta) = e.metadata() {
                    let s = meta.len();
                    scanned.fetch_add(1, Ordering::Relaxed);
                    bytes.fetch_add(s, Ordering::Relaxed);
                    if s >= opts.min_size {
                        seed.push(FileInfo {
                            path: p.to_string_lossy().to_string(),
                            size: s,
                            modified: fmt_modified(&meta),
                            depth: 0,
                            is_dir: false,
                            is_system: false,
                        });
                    }
                }
            }
        }
    }

    let progress = Progress {
        app,
        stage: "scanning",
        scanned: &scanned,
        bytes: &bytes,
        errors: &errors,
        start,
        last_emit: std::sync::Mutex::new(Instant::now()),
    };

    let mut items: Vec<FileInfo> = subdir_paths
        .par_iter()
        .flat_map(|d| {
            let mut local: Vec<FileInfo> = Vec::new();
            for entry in walkdir::WalkDir::new(d).follow_links(false).into_iter().filter_entry(|e| {
                let p = e.path();
                let nm = file_name(p);
                if !opts.include_hidden && p != d {
                    if let Ok(m) = p.metadata() {
                        if is_hidden(&m) { return false; }
                    }
                }
                if opts.skip_system && is_system_name(&nm) && p != d { return false; }
                true
            }) {
                let entry = match entry { Ok(x) => x, Err(_) => { errors.fetch_add(1, Ordering::Relaxed); continue; } };
                let p = entry.path();
                if p == d { continue; }
                let ft = entry.file_type();
                if ft.is_dir() { continue; }
                if !ft.is_file() { continue; }
                if let Ok(meta) = entry.metadata() {
                    let s = meta.len();
                    scanned.fetch_add(1, Ordering::Relaxed);
                    bytes.fetch_add(s, Ordering::Relaxed);
                    if s >= opts.min_size {
                        local.push(FileInfo {
                            path: p.to_string_lossy().to_string(),
                            size: s,
                            modified: fmt_modified(&meta),
                            depth: entry.depth(),
                            is_dir: false,
                            is_system: is_system_name(&file_name(p)),
                        });
                    }
                    progress.tick(&p.to_string_lossy());
                }
            }
            local
        })
        .collect();

    items.extend(seed);
    items.sort_by(|a, b| b.size.cmp(&a.size));
    let total_size = items.iter().map(|i| i.size).sum();
    if items.len() > 5000 {
        items.truncate(5000);
    }

    BigFilesResult {
        root: root.to_string(),
        items,
        total_size,
        duration_ms: start.elapsed().as_millis(),
        skipped_errors: errors.load(Ordering::Relaxed),
    }
}

/// 收集所有参与重复检测的候选文件（size >= threshold，跳过系统/隐藏）
/// 直接使用全量版本（不截断），保证不会遗漏较小但仍达标的候选
pub fn collect_dup_candidates(
    app: &AppHandle,
    root: &str,
    threshold: u64,
    skip_system: bool,
    include_hidden: bool,
) -> Vec<FileInfo> {
    let opts = ScanOpts {
        min_size: threshold,
        skip_system,
        include_hidden,
        top: usize::MAX,
        with_big_files: false,
    };
    scan_big_files_full(app, root, &opts).items
}

/// 不截断的版本，用于重复检测
fn scan_big_files_full(app: &AppHandle, root: &str, opts: &ScanOpts) -> BigFilesResult {
    let start = Instant::now();
    // 仅简化实现：直接走带系统跳过的 walkdir 全盘收集
    let scanned = AtomicU64::new(0);
    let bytes = AtomicU64::new(0);
    let errors = AtomicU64::new(0);
    let mut items: Vec<FileInfo> = Vec::new();
    let progress = Progress { app, stage: "scanning", scanned: &scanned, bytes: &bytes, errors: &errors, start, last_emit: std::sync::Mutex::new(Instant::now()) };
    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
        let p = e.path();
        let nm = file_name(p);
        if !opts.include_hidden && p != Path::new(root) {
            if let Ok(m) = p.metadata() { if is_hidden(&m) { return false; } }
        }
        if opts.skip_system && is_system_name(&nm) && p != Path::new(root) { return false; }
        true
    }) {
        let entry = match entry { Ok(x) => x, Err(_) => { errors.fetch_add(1, Ordering::Relaxed); continue; } };
        if !entry.file_type().is_file() { continue; }
        let p = entry.path();
        if p == Path::new(root) { continue; }
        if let Ok(meta) = entry.metadata() {
            let s = meta.len();
            scanned.fetch_add(1, Ordering::Relaxed);
            bytes.fetch_add(s, Ordering::Relaxed);
            if s >= opts.min_size {
                items.push(FileInfo {
                    path: p.to_string_lossy().to_string(),
                    size: s,
                    modified: fmt_modified(&meta),
                    depth: entry.depth(),
                    is_dir: false,
                    is_system: is_system_name(&file_name(p)),
                });
            }
            progress.tick(&p.to_string_lossy());
        }
    }
    items.sort_by(|a, b| b.size.cmp(&a.size));
    BigFilesResult {
        root: root.to_string(),
        total_size: items.iter().map(|i| i.size).sum(),
        items,
        duration_ms: start.elapsed().as_millis(),
        skipped_errors: errors.load(Ordering::Relaxed),
    }
}

/// 构建按 size 分组的候选 hash map（公开给 hasher 使用）
pub fn group_by_size(files: Vec<FileInfo>) -> HashMap<u64, Vec<FileInfo>> {
    let mut m: HashMap<u64, Vec<FileInfo>> = HashMap::new();
    for f in files {
        m.entry(f.size).or_default().push(f);
    }
    m
}