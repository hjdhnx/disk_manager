use serde::{Deserialize, Serialize};

/// 扫描选项（前端可选传，缺省用默认值）
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScanOpts {
    /// 大文件最小字节数阈值，默认 100MB
    pub min_size: u64,
    /// 是否跳过系统目录（Windows、Program Files 等），概览/大文件默认 true
    pub skip_system: bool,
    /// 是否包含隐藏文件，默认 true
    pub include_hidden: bool,
    /// 概览每层返回的最大条目数，默认 40
    pub top: usize,
    /// 是否在下钻概览时同时返回该层的大文件，默认 false
    pub with_big_files: bool,
}

impl Default for ScanOpts {
    fn default() -> Self {
        Self {
            min_size: 100 * 1024 * 1024,
            skip_system: true,
            include_hidden: true,
            top: 40,
            with_big_files: false,
        }
    }
}

/// 盘符信息
#[derive(Debug, Clone, Serialize)]
pub struct DiskInfo {
    pub letter: String,   // 如 "C:\\"
    pub name: String,     // 卷标/名称
    pub mount: String,    // 挂载点
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub is_system: bool,
}

/// 概览中的一个目录项（直接子目录聚合 + 该层自己的文件大小）
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub is_system: bool,
    /// 该目录下落地的"本层大文件"(只在 with_big_files=true 时)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub big_files: Vec<FileInfo>,
}

/// 概览扫描结果摘要
#[derive(Debug, Clone, Serialize)]
pub struct SpaceSummary {
    pub root: String,
    pub total_bytes: u64,        // 整盘盘符容量（来自 DiskInfo），若下钻则为 0
    pub used_bytes: u64,         // 整盘已用
    pub scanned_bytes: u64,     // 本次实际累计到的大小
    pub file_count: u64,
    pub dir_count: u64,
    pub duration_ms: u128,
    pub skipped_errors: u64,
    pub dirs: Vec<DirEntry>,     // 直接子目录聚合（按 size 降序，截断 top）
    /// 当前 root 自己的直接"本层文件"（with_big_files=true 且 size>=min_size 时填充）
    #[serde(default)]
    pub root_files: Vec<FileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub modified: String,   // RFC3339
    pub depth: usize,
    pub is_dir: bool,
    pub is_system: bool,
}

impl Default for FileInfo {
    fn default() -> Self {
        Self {
            path: String::new(),
            size: 0,
            modified: String::new(),
            depth: 0,
            is_dir: false,
            is_system: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BigFilesResult {
    pub root: String,
    pub items: Vec<FileInfo>,
    pub total_size: u64,
    pub duration_ms: u128,
    pub skipped_errors: u64,
}

/// 一个重复文件组
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DupGroup {
    pub md5: String,
    pub size: u64,
    /// 文件列表（包含所有副本）
    pub files: Vec<FileInfo>,
    /// 建议保留的索引（路径最浅 + 最近修改优先）
    pub keep_index: usize,
    /// 该组浪费的字节数 = (count-1)*size
    pub waste: u64,
}

impl Default for DupGroup {
    fn default() -> Self {
        Self {
            md5: String::new(),
            size: 0,
            files: Vec::new(),
            keep_index: 0,
            waste: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DupResult {
    pub root: String,
    pub groups: Vec<DupGroup>,
    pub total_waste: u64,
    pub duration_ms: u128,
    pub scanned_files: u64,
    pub hashed_files: u64,
}

impl Default for DupResult {
    fn default() -> Self {
        Self {
            root: String::new(),
            groups: Vec::new(),
            total_waste: 0,
            duration_ms: 0,
            scanned_files: 0,
            hashed_files: 0,
        }
    }
}

/// 扫描进度事件 payload
#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub stage: String,   // "scanning" | "hashing" | "done" | "error"
    pub current: String, // 当前正在处理的路径
    pub scanned: u64,    // 已扫描条目数
    pub hashed: u64,     // 已计算 hash 的文件数
    pub bytes: u64,      // 已累计字节
    pub elapsed_ms: u128,
}

/// 删除返回
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanItem {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanResult {
    pub succeeded: Vec<CleanItem>,
    pub failed: Vec<CleanItem>,
    pub freed_bytes: u64,
}

/// 清理记录条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub time: String,    // RFC3339
    pub action: String,  // "trash" | "delete"
    pub freed_bytes: u64,
    pub items: Vec<CleanItem>,
}