import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

// 磁盘：列出所有可用盘符及容量
export const listDisks = () => invoke('list_disks')

// 扫描：开始扫描磁盘的空间占用（目录聚合），返回扫描摘要
// root: 盘符或路径，如 "C:\\"；扫描进度由 scan_progress 事件推送
export const scanSpace = (root, opts) =>
  invoke('scan_space', { root, opts })

// 扫描大文件：返回超过阈值的大文件列表，进度由 scan_progress 事件推送
export const scanBigFiles = (root, opts) =>
  invoke('scan_big_files', { root, opts })

// 计算重复文件：对给定文件列表计算 MD5 并分组，返回重复组
export const findDuplicates = (root, opts) =>
  invoke('find_duplicates', { root, opts })

// 刷新已有重复组：只检查文件是否存在，过滤已删除的，不重算 MD5
export const refreshDuplicates = (result) =>
  invoke('refresh_duplicates', { result })

// 删除文件到回收站（可恢复）
export const trashFiles = (paths) => invoke('trash_files', { paths })

// 永久删除（谨慎）
export const deleteFiles = (paths) => invoke('delete_files', { paths })

// 打开文件所在的目录（资源管理器定位）
export const openInExplorer = (path) => invoke('open_in_explorer', { path })

// 保存导出文本到应用数据目录下的 exports/
export const saveExport = (filename, content) =>
  invoke('save_export', { filename, content })

// 读取历史清理记录
export const getCleanHistory = () => invoke('get_clean_history')

// 清空清理记录
export const clearCleanHistory = () => invoke('clear_clean_history')

// 监听扫描进度事件
export const onScanProgress = (handler) =>
  listen('scan_progress', (e) => handler(e.payload))

// 监听扫描完成事件
export const onScanDone = (handler) =>
  listen('scan_done', (e) => handler(e.payload))