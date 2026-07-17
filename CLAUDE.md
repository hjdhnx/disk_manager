# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Tauri 2 + Rust + Vue 3 的桌面应用，帮助用户看清磁盘空间占用、找出大文件与 MD5 重复文件，并把它们一键清理到回收站。前端 Vue 3 + Vite（产物到 `dist/`），通过 `@tauri-apps/api` 的 `invoke` 和事件监听桥接后端。清理记录保存到 `clean_history.json`（小量历史，无需 SQLite）。

## 常用命令

```bash
pnpm install                          # 安装前端 + @tauri-apps/cli（含 esbuild 构建）
pnpm tauri dev                        # 开发：cargo build debug + vite + 启动窗口
pnpm tauri build                      # 打包 msi/nsis（产物在 src-tauri/target/release/bundle/）
cargo check --manifest-path src-tauri/Cargo.toml   # 仅类型检查（比 dev 快，不链接不启动）
pnpm build                            # 仅前端构建
```

无自动化测试、无 lint。验证靠 `pnpm tauri dev` 启动后手动测试。

**开发循环**：改 `src/*`（Vue）→ Vite 热重载；改 `src-tauri/src/*` 或 `Cargo.toml` → tauri dev 自动重编译并重启窗口。窗口运行中重建 exe 前先杀残留进程（`taskkill //F //IM disk-manager.exe`），否则链接报 `os error 5`。

## 关键架构

### 1. 扫描平行化：rayon + walkdir

`scanner.rs` 对"磁盘根的直接子目录"做 `read_dir`，再用 `rayon::par_iter` 并行 `walkdir` 每个子目录聚合，避免单个 `walkdir` 全串行。这是概览面板几秒出结果的关键。改 `aggregate_subdir` / `scan_big_files` 时要注意：所有累计计数用 `AtomicU64`，进度事件用 `AppHandle::emit("scan_progress", ...)` 并 120ms 节流。

### 2. 系统目录识别（`SYSTEM_NAMES`）

`scanner.rs::SYSTEM_NAMES` 列出 Windows / Program Files / $Recycle.Bin 等。`scan_space` / `scan_big_files` 默认 `skip_system=true` 跳过这些目录，避免长耗时和无意义结果（如 WinSxS 海量小文件）。改这张表前理解：它直接决定扫描覆盖率，加错会导致系统目录被扫进去或漏扫用户数据目录。

### 3. MD5 重复识别的两级指纹（`hasher.rs`）

1. `collect_dup_candidates` 收集 `size >= threshold` 的文件
2. 按 `size` 分组，只对 `count >= 2` 的组继续
3. 计算 `head_md5`（前 16KB）二次分组
4. 仅对 head 也相同的候选计算 `full_md5`（流式 64KB 分块）

这样把 O(N) 全文件 hash 降到只在"很可能重复"的组内做完整 hash。改流程时不要先 full hash 再分组——会慢到不可用。

### 4. 重复组的保留建议（`keep_index`）

`hasher.rs` 对每组重复文件按 `depth asc, modified desc` 排序后取索引 0 为"保留"项：路径最浅 + 修改最新优先。前端展示"保留/副本"标签，用户批量删除时默认排除 keep_index 项。改排序规则要同步更新 `DuplicatePanel.vue` 的 UI 提示。

### 5. 删除走 trash crate（可恢复）

`cleaner.rs::trash_files` 用 `trash::delete`，文件进系统回收站，**可恢复**。`delete_files` 是永久删除（默认未在 UI 暴露，仅命令）。每次成功删除会 `History::append` 一条记录到 `%LOCALAPPDATA%\com.taoint.diskmemory\clean_history.json`，最近 500 条。

### 6. Tauri ↔ 前端桥接约定

- 后端命令在 `commands.rs`，`#[tauri::command]`；`State<'_, T>` 由 Tauri 自动注入，前端**不传**
- 前端 `import { invoke } from '@tauri-apps/api/core'`，参数 key 与 Rust 参数名（snake_case）一致
- 扫描进度通过 `app.emit("scan_progress", ScanProgress)` 推送，前端 `listen('scan_progress', ...)` 接收
- 错误统一以 `Result<T, String>` 返回，前端 `try/catch`
- 加新命令必须在 `lib.rs::generate_handler![...]` 注册

## 必须知道的约束与坑

### `withGlobalTauri: false` + 用 import api

前端用 `@tauri-apps/api/core` 的 `invoke` 和 `@tauri-apps/api/event` 的 `listen`，不靠 `window.__TAURI__`。capabilities 给 `core:default` + `core:event:default` 即可（事件订阅已包含）。

### `beforeDevCommand` / `beforeBuildCommand` 用 `pnpm`

`tauri.conf.json` 的构建钩子是 `pnpm dev` / `pnpm build`（不是 npm）。换包管理器需同步改。

### pnpm 11 不读 `package.json` 的 `pnpm` 字段

设置在 `pnpm-workspace.yaml`（`allowBuilds: esbuild: true` + `onlyBuiltDependencies: [esbuild]`）。删或改错会导致 `tauri` 前置的依赖检查报 `ERR_PNPM_IGNORED_BUILDS`，Vite 无法构建。

### MSI 打包中文 productName 必须设 wix `language: "zh-CN"`

`productName` 含中文（「磁盘清理帮手」），WiX 默认用 code page 1252 编码不了中文，报 `LGHT0311`。`tauri.conf.json` 的 `bundle.windows.wix.language` 已设 `"zh-CN"`。NSIS 不受影响。

### `tauri-build` 在 Windows 必须有 `icons/icon.ico`

缺图标，cargo check/build 在 build script 阶段就报 ``icons/icon.ico` not found``。当前 `icon.ico` 是从 alias_manager 复制的占位图标，正式发布前应用 `pnpm tauri icon app-icon.png` 生成专属图标。

### explorer `/select,"path"` 用 `CommandExt::raw_arg`

`commands.rs::open_in_explorer` 对存在的文件用 `explorer.exe /select,<path>` 定位。`/select,` 后内含路径分隔符和可能的空格，普通 `.arg()` 会再加一层引号导致定位失败，必须用 `std::os::windows::process::CommandExt::raw_arg`。纯目录路径只需 `.arg()`。

### 大文件全量收集的截断坑

`scan_big_files` 会截断到 5000 项（前端显示性能保护）。但 `collect_dup_candidates` 需要无遗漏的候选集去算 MD5，所以它改调 `scan_big_files_full`（不截断）。改这两个函数时不要让 `collect_dup_candidates` 走截断版本，否则会漏检重复。

### 进度事件的节流

`Progress::tick` 每 120ms 最多 emit 一次 `scan_progress`。改这个间隔要注意：太小会让 IPC 风暴拖慢扫描；太大会让进度条卡顿。120ms 经验值。

### 安静的 `errors` 字段

`scanner.rs::Progress` 结构体的 `errors: &'a AtomicU64` 字段当前在 `tick` 里未使用（错误计数在调用方累计）。保留是为了将来在进度里直接显示"已跳过 X 个错误"。删它会改善 dead_code warning，但留着更易扩展。

## 配置落点

- 数据/程序目录：`%LOCALAPPDATA%\com.taoint.diskmanager\`（由 `tauri.conf.json` 的 `identifier` 决定）
- `clean_history.json`：清理历史记录