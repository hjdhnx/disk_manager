<script setup>
import { ref, inject, watch, onMounted, onUnmounted } from 'vue'
import * as api from '../api.js'

const props = defineProps({
  root: String,
  summary: Object,
  scanning: Boolean,
  currentDisk: Object,
  opts: Object,
})
const emit = defineEmits(['scan', 'toast', 'refresh-disks'])

const fmtSize = inject('fmtSize')
const fmtMs = inject('fmtMs')

// 下钻路径栈：基栈 = root，每下钻一层 push 一个目录聚合
const stack = ref([]) // [{ path, dirs: DirEntry[], subDirs: {} }] 缓存
const cur = ref(null) // 当前层级 { path, dirs }

// 本地"子扫描"状态（下钻 / 刷新当前目录时显示进度，独立于 App 顶层的全局 scanning）
const subScanning = ref(false)
const subProgress = ref(null) // { stage, current, scanned, bytes, elapsed_ms }
let unlistenProgress = null

watch(
  () => props.summary,
  (s) => {
    if (!s) return
    stack.value = [{ path: s.root, dirs: s.dirs, rootFiles: s.root_files || [] }]
    cur.value = stack.value[0]
  },
  { immediate: true }
)

async function drill(entry) {
  if (props.scanning || subScanning.value) return
  subScanning.value = true
  subProgress.value = { stage: 'scanning', current: '', scanned: 0, bytes: 0, elapsed_ms: 0 }
  emit('toast', '正在扫描 ' + entry.name + ' ...', 'info')
  try {
    const sub = await api.scanSpace(entry.path, props.opts)
    stack.value.push({ path: sub.root, dirs: sub.dirs, rootFiles: sub.root_files || [] })
    cur.value = stack.value[stack.value.length - 1]
  } catch (e) {
    emit('toast', '下钻失败：' + e, 'error')
  } finally {
    subScanning.value = false
    subProgress.value = null
  }
}

async function refreshCurrent() {
  if (props.scanning || subScanning.value || !cur.value) return
  subScanning.value = true
  subProgress.value = { stage: 'scanning', current: '', scanned: 0, bytes: 0, elapsed_ms: 0 }
  emit('toast', '正在重新扫描当前目录 ...', 'info')
  try {
    const sub = await api.scanSpace(cur.value.path, props.opts)
    cur.value = { path: sub.root, dirs: sub.dirs, rootFiles: sub.root_files || [] }
    stack.value = stack.value.slice(0, stack.value.length)
    stack.value[stack.value.length - 1] = cur.value
  } catch (e) {
    emit('toast', '刷新失败：' + e, 'error')
  } finally {
    subScanning.value = false
    subProgress.value = null
  }
}

function gotoLevel(i) {
  if (i < 0 || i >= stack.value.length) return
  stack.value = stack.value.slice(0, i + 1)
  cur.value = stack.value[stack.value.length - 1]
}

async function openInExplorer(p) {
  try { await api.openInExplorer(p) } catch (e) { emit('toast', '打开失败：' + e, 'error') }
}

async function trashSelects(paths) {
  for (const p of paths) {
    if (!confirm(`将删除到回收站：\n${p}\n继续？`)) return
  }
  try {
    const r = await api.trashFiles(paths)
    emit('toast', `已清理 ${fmtSize(r.freed_bytes)}`)
    emit('refresh-disks')
  } catch (e) {
    emit('toast', '删除失败：' + e, 'error')
  }
}

function sortedDirs() {
  if (!cur.value) return []
  return cur.value.dirs.slice().sort((a, b) => b.size - a.size)
}

const totalScanned = () => props.summary?.scanned_bytes || 0

function shortName(p) {
  const parts = p.split(/[\\/]/)
  return parts[parts.length - 1] || p
}
function dirOf(p) {
  const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'))
  return i > 0 ? p.slice(0, i) : p
}

// 监听扫描进度事件，下钻/刷新时更新本地进度
onMounted(async () => {
  unlistenProgress = await api.onScanProgress((p) => {
    if (subScanning.value) subProgress.value = p
  })
})
onUnmounted(() => {
  if (unlistenProgress) unlistenProgress()
})
</script>

<template>
  <div>
    <!-- 盘使用卡 -->
    <div v-if="currentDisk" class="disk-cards" style="margin-bottom:16px">
      <div :class="['disk-card']">
        <div class="disk-name">{{ currentDisk.mount }} <span style="font-size:12px;color:var(--text-3);font-weight:normal">{{ currentDisk.is_system ? '(系统盘)' : '' }}</span></div>
        <div class="disk-mount">{{ currentDisk.name }}</div>
        <div class="bar"><div :style="{ width: (currentDisk.total ? currentDisk.used / currentDisk.total * 100 : 0) + '%' }"></div></div>
        <div class="disk-num">
          <span>已用 {{ fmtSize(currentDisk.used) }} / 共 {{ fmtSize(currentDisk.total) }}</span>
          <span class="used">{{ currentDisk.total ? (currentDisk.used / currentDisk.total * 100).toFixed(1) : 0 }}%</span>
        </div>
      </div>
    </div>

    <div class="toolbar">
      <button class="btn primary sm" @click="emit('scan')" :disabled="scanning || subScanning">
        {{ scanning ? '扫描中...' : '一键扫描磁盘占用' }}
      </button>
      <button class="btn ghost sm" @click="refreshCurrent" :disabled="scanning || subScanning" v-if="cur">
        {{ subScanning ? '刷新中...' : '刷新当前目录' }}
      </button>
      <span style="font-size:12px;color:var(--text-3)" v-if="summary">
        已扫 {{ summary.file_count.toLocaleString() }} 文件 / {{ summary.dir_count.toLocaleString() }} 目录 · 累计 {{ fmtSize(totalScanned()) }} · 用时 {{ fmtMs(summary.duration_ms) }}
      </span>
    </div>

    <!-- 面包屑 -->
    <div class="breadcrumb" v-if="stack.length">
      <a @click="gotoLevel(0)">{{ summary?.root }}</a>
      <template v-for="(s, i) in stack.slice(1)" :key="i">
        <span class="sep">›</span>
        <a @click="gotoLevel(i + 1)">{{ s.path.split(/[\\/]/).pop() || s.path }}</a>
      </template>
    </div>

    <div v-if="!summary && !scanning" class="empty">
      <p class="empty-title">尚未扫描</p>
      <p class="empty-desc">点击上方"一键扫描"按钮，查看磁盘空间占用分布</p>
    </div>

    <template v-else-if="summary">
      <div style="display:flex;align-items:baseline;justify-content:space-between;margin:4px 0 8px">
        <div style="font-size:13px;color:var(--text-2);font-weight:600">
          📂 当前目录: <span class="mono" style="color:var(--text)">{{ cur?.path }}</span>
          <span style="margin-left:8px;color:var(--text-3);font-weight:normal">点击下列目录行可继续下钻 ›</span>
        </div>
      </div>

      <!-- 本层大文件 -->
      <div v-if="cur?.rootFiles?.length" style="margin-bottom:16px">
        <div style="font-size:12px;color:var(--text-3);margin-bottom:6px">本层大文件（{{ cur.rootFiles.length }} 个 ≥ {{ fmtSize(opts.min_size) }}）</div>
        <div class="table">
          <div class="row head" style="grid-template-columns: 1fr 110px 150px 80px">
            <span>文件路径</span><span>大小</span><span>修改时间</span><span>操作</span>
          </div>
          <div v-for="f in cur.rootFiles" :key="f.path" class="row" style="grid-template-columns: 1fr 110px 150px 80px" :class="{ system: f.is_system }">
            <span class="mono" :title="f.path">📄 {{ shortName(f.path) }}<div style="font-size:11px;color:var(--text-3)">{{ dirOf(f.path) }}</div></span>
            <span style="color:var(--primary);font-weight:600">{{ fmtSize(f.size) }}</span>
            <span style="color:var(--text-2);font-size:12px">{{ f.modified.replace('T', ' ').slice(0, 19) }}</span>
            <span><button class="btn ghost sm" @click="openInExplorer(f.path)">定位</button></span>
          </div>
        </div>
      </div>

      <!-- 子目录列表 -->
      <div v-if="sortedDirs().length" class="table">
        <div class="row head" style="grid-template-columns: 1fr 110px 90px 90px 110px 80px">
          <span>目录（可下钻）</span><span>占用大小</span><span>文件数</span><span>子目录数</span><span>占比</span><span>操作</span>
        </div>
        <div
          v-for="d in sortedDirs()"
          :key="d.path"
          :class="['row clickable', { system: d.is_system }]"
          style="grid-template-columns: 1fr 110px 90px 90px 110px 80px"
          @click="drill(d)"
        >
          <span class="mono" :title="d.path">
            <span style="color:var(--primary)">▸</span> 📁 {{ d.name }}{{ d.is_system ? ' [系统]' : '' }}
          </span>
          <span style="color:var(--primary);font-weight:600">{{ fmtSize(d.size) }}</span>
          <span style="color:var(--text-2)">{{ d.file_count.toLocaleString() }}</span>
          <span style="color:var(--text-2)">{{ d.dir_count.toLocaleString() }}</span>
          <span style="color:var(--text-3)">{{ summary.scanned_bytes ? (d.size / summary.scanned_bytes * 100).toFixed(2) + '%' : '-' }}</span>
          <span @click.stop>
            <button class="btn ghost sm" @click="openInExplorer(d.path)">打开</button>
          </span>
        </div>
      </div>

      <div v-else class="empty">
        <p class="empty-title">该路径下没有可统计的子目录</p>
        <p class="empty-desc">可能权限受限或为叶子目录；上方"本层大文件"区列出了该目录直属大文件。</p>
      </div>
    </template>

    <!-- 下钻/刷新当前目录时的底部进度条（fixed bottom，与 App 顶层根扫描进度条共用同位置/样式，但两者不会同时触发）-->
    <div v-if="subScanning" class="progress">
      <span class="label">{{ subProgress?.stage === 'hashing' ? '计算MD5' : '下钻扫描' }}</span>
      <div class="pbar"><div></div></div>
      <span class="stat" :title="subProgress?.current || ''">
        {{ (subProgress?.scanned || 0).toLocaleString() }} 项 · {{ fmtSize(subProgress?.bytes || 0) }}
        <span style="margin-left:6px;color:var(--text-2)">{{ subProgress?.current ? subProgress.current.split(/[\\/]/).pop() : '' }}</span>
      </span>
    </div>
  </div>
</template>