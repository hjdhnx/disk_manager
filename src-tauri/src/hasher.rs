use crate::models::{DupGroup, DupResult, FileInfo};
use crate::scanner;
use md5::{Digest, Md5};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

const HEAD_BYTES: usize = 16 * 1024; // 先算前 16KB 指纹

/// 计算文件前 HEAD_BYTES 字节的 MD5（快速指纹）
fn head_md5(path: &str) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let mut hasher = Md5::new();
    let mut buf = [0u8; HEAD_BYTES];
    let n = f.read(&mut buf).ok()?;
    if n == 0 {
        // 空文件：用全零参与分组
        hasher.update(&[0u8; 16]);
    } else {
        hasher.update(&buf[..n]);
    }
    Some(hex::encode(hasher.finalize()))
}

/// 计算文件完整 MD5（流式分块）
fn full_md5(path: &str) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let mut hasher = Md5::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return None,
        };
        hasher.update(&buf[..n]);
    }
    Some(hex::encode(hasher.finalize()))
}

/// 主入口：对 root 下 size >= threshold 的文件做重复检测
pub fn find_duplicates(
    app: &AppHandle,
    root: &str,
    threshold: u64,
    skip_system: bool,
    include_hidden: bool,
) -> DupResult {
    let start = Instant::now();
    let hashed = AtomicU64::new(0);
    let scanned_files = AtomicU64::new(0);

    // 1) 收集候选
    let candidates = scanner::collect_dup_candidates(app, root, threshold, skip_system, include_hidden);

    // 2) 按 size 分组，保留 count>=2 的
    let by_size: HashMap<u64, Vec<FileInfo>> = scanner::group_by_size(candidates);
    let dup_size_groups: Vec<(u64, Vec<FileInfo>)> = by_size
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .collect();
    scanned_files.store(
        dup_size_groups.iter().map(|(_, v)| v.len() as u64).sum(),
        Ordering::Relaxed,
    );

    // 3) 对每个 size 组计算 head MD5，按 head 再分组
    // file_id -> head md5
    struct Cand {
        info: FileInfo,
        head: Option<String>,
    }
    let cands: Vec<Cand> = dup_size_groups
        .par_iter()
        .flat_map(|(_size, files)| {
            files
                .par_iter()
                .map(|f| {
                    let h = head_md5(&f.path);
                    Cand { info: f.clone(), head: h }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // 4) 对 head 相同且同 size & count>=2 的，计算完整 MD5
    // 按 (size, head) 分组
    let mut head_groups: HashMap<(u64, String), Vec<Cand>> = HashMap::new();
    for c in cands {
        if let Some(h) = &c.head {
            head_groups.entry((c.info.size, h.clone())).or_default().push(c);
        }
    }

    // 候选真正参与完整 hash 的文件
    let to_full: Vec<&Cand> = head_groups
        .values()
        .filter(|g| g.len() >= 2)
        .flat_map(|g| g.iter())
        .collect();

    // 计算完整 MD5
    let full: Vec<(usize, String)> = to_full
        .par_iter()
        .enumerate()
        .map(|(i, c)| {
            let m = full_md5(&c.info.path).unwrap_or_default();
            hashed.fetch_add(1, Ordering::Relaxed);
            (i, m)
        })
        .collect();

    // 回填完整 md5
    let mut full_map: HashMap<usize, String> = HashMap::new();
    for (i, m) in full {
        full_map.insert(i, m);
    }
    let idx_seq: Vec<usize> = to_full.iter().enumerate().map(|(i, _)| i).collect();
    let mut info_groups: HashMap<(u64, String), Vec<FileInfo>> = HashMap::new();
    for (slot, i) in idx_seq.iter().enumerate() {
        let i = *i;
        let cand = &to_full[slot];
        if let Some(m) = full_map.get(&i) {
            info_groups
                .entry((cand.info.size, m.clone()))
                .or_default()
                .push(cand.info.clone());
        }
    }

    // 5) 生成重复组
    let mut groups: Vec<DupGroup> = info_groups
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .map(|((size, md5), mut files)| {
            // 保留：路径最浅 + 修改时间最新
            files.sort_by(|a, b| {
                a.depth
                    .cmp(&b.depth)
                    .then(b.modified.cmp(&a.modified))
            });
            let keep_index = 0;
            let waste = (files.len() as u64 - 1) * size;
            // emit 周期性 hashing 进度
            let _ = app.emit(
                "scan_progress",
                crate::models::ScanProgress {
                    stage: "hashing".to_string(),
                    current: files.first().map(|f| f.path.clone()).unwrap_or_default(),
                    scanned: scanned_files.load(Ordering::Relaxed),
                    hashed: hashed.load(Ordering::Relaxed),
                    bytes: 0,
                    elapsed_ms: start.elapsed().as_millis(),
                },
            );
            DupGroup {
                md5,
                size,
                files,
                keep_index,
                waste,
            }
        })
        .collect();

    groups.sort_by(|a, b| b.waste.cmp(&a.waste));
    let total_waste: u64 = groups.iter().map(|g| g.waste).sum();

    DupResult {
        root: root.to_string(),
        groups,
        total_waste,
        duration_ms: start.elapsed().as_millis(),
        scanned_files: scanned_files.load(Ordering::Relaxed),
        hashed_files: hashed.load(Ordering::Relaxed),
    }
}