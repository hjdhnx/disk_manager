# 磁盘清理帮手

Tauri 2 + Rust + Vue 3 的桌面应用，帮助用户看清磁盘空间占用、找出大文件与 MD5 重复文件，并把它们一键清理到回收站。

## 功能

- 磁盘概览：列出所有盘符使用情况，默认 C 盘；按直接子目录聚合并排序，支持下钻
- 大文件扫描：一键列出磁盘上超过阈值（默认 100MB）的大文件，可批量删除到回收站
- 重复文件识别：基于 MD5 两级指纹（前 16KB 头 → 完整 MD5）分组重复文件，给出"保留/副本"建议
- 清理记录：记录最近 500 次清理操作，可清空
- 安全删除：默认进回收站（可恢复），另提供永久删除命令（未在 UI 暴露）

## 技术栈

| 层 | 技术 |
|----|------|
| 前端 | Vue 3 + Vite 6 + @tauri-apps/api 2 |
| 后端 | Rust 2021 + Tauri 2 |
| 扫描 | rayon（并行）+ walkdir（递归） |
| 哈希 | md-5（两级指纹，流式 64KB 分块） |
| 删除 | trash crate（系统回收站，可恢复） |
| 磁盘信息 | sysinfo（盘符/容量） |
| 包管理 | pnpm 11 |

## 目录结构

```
disk_manager/
├── src/                      # Vue 前端
│   ├── App.vue               # 主入口 + 标签切换
│   ├── api.js                # invoke 封装
│   ├── components/
│   │   ├── OverviewPanel.vue      # 磁盘概览（可下钻）
│   │   ├── BigFilesPanel.vue      # 大文件列表（可批量删）
│   │   ├── DuplicatePanel.vue     # 重复组（保留/副本标签）
│   │   └── HistoryPanel.vue       # 清理记录
│   └── styles.css
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/default.json
    ├── icons/icon.ico         # 占位（后续 pnpm tauri icon 替换）
    └── src/
        ├── main.rs
        ├── lib.rs             # Tauri Builder + generate_handler 注册
        ├── models.rs          # 前后交互的 Serde 数据模型
        ├── scanner.rs         # 空间占用 + 大文件扫描（rayon+walkdir）
        ├── hasher.rs          # MD5 两级指纹重复识别
        ├── cleaner.rs         # trash / delete
        ├── history.rs         # clean_history.json 读写
        └── commands.rs        # #[tauri::command] 命令实现
```

## 快速开始

```bash
pnpm install
pnpm tauri dev        # 开发模式（热重载）
pnpm tauri build      # 打包 msi/nsis
```

## 设计要点

### 扫描并行化

`scanner.rs` 对磁盘根目录的直接子目录做 `read_dir`，再用 `rayon::par_iter` 并行 `walkdir` 聚合每个子目录。进度事件 `scan_progress` 通过 Tauri 事件推送，120ms 节流。

### MD5 两级指纹

1. 按 size 分组，只保留 count >= 2 的组
2. 计算 head MD5（前 16KB）二次分组，排除不同内容但同大小的文件
3. 仅对 head 也相同的候选计算完整 MD5

将 O(N) 全文件哈希降到只在"很可能重复"的小组内完成，避免全盘哈希带来的不可用延迟。

### 保留建议

每组重复按 `depth asc, modified desc` 排序取首位为"保留"项：路径最浅 + 修改最新优先。前端用绿色"保留"/橙色"副本"标签区分，用户批量删除时默认排除保留项。

### 安全删除

`trash::delete` 走系统回收站，恢复能力。
每次成功删除会写一条 `HistoryEntry` 到
`%LOCALAPPDATA%\com.taoint.diskmanager\clean_history.json`（最近 500 条）。

## 许可

MIT