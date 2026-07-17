<script setup>
import { ref, inject, computed } from 'vue'
import * as api from '../api.js'

const props = defineProps({
  root: String,
  data: Object,
  scanning: Boolean,
  opts: Object
})
const emit = defineEmits(['scan', 'toast'])

const fmtSize = inject('fmtSize')
const fmtMs = inject('fmtMs')

const keyword = ref('')
const selected = ref(new Set())

const filtered = computed(() => {
  const k = keyword.value.trim().toLowerCase()
  if (!k) return props.data?.items || []
  return (props.data?.items || []).filter(i => i.path.toLowerCase().includes(k))
})

const totalSelected = computed(() => {
  const items = props.data?.items || []
  let s = 0
  for (const i of items) if (selected.value.has(i.path)) s += i.size
  return s
})

function toggle(p, checked) {
  const n = new Set(selected.value)
  if (checked) n.add(p); else n.delete(p)
  selected.value = n
}
function selectAll() {
  const n = new Set()
  filtered.value.forEach(i => n.add(i.path))
  selected.value = n
}
function clearSel() { selected.value = new Set() }

async function trashSelected() {
  const paths = Array.from(selected.value)
  if (!paths.length) return
  if (!confirm(`确认将选中的 ${paths.length} 个文件（共 ${fmtSize(totalSelected.value)}）删除到回收站？\n可从回收站恢复。`)) return
  try {
    const r = await api.trashFiles(paths)
    emit('toast', `已清理 ${r.succeeded.length} 个文件，释放 ${fmtSize(r.freed_bytes)}`)
    selected.value = new Set()
    if (!props.scanning) emit('scan')
  } catch (e) {
    emit('toast', '删除失败：' + e, 'error')
  }
}

async function openInExplorer(p) {
  try { await api.openInExplorer(p) } catch (e) { emit('toast', '打开失败：' + e, 'error') }
}

function shortName(p) {
  const parts = p.split(/[\\/]/)
  return parts[parts.length - 1] || p
}
function dirOf(p) {
  const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'))
  return i > 0 ? p.slice(0, i) : p
}

function humanSize(n) {
  if (!n) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0, v = n
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${u[i]}`
}

async function exportList() {
  const list = filtered.value
  if (!list.length) return
  try {
    const lines = []
    lines.push(`# 磁盘大文件清单（${props.root}）`)
    lines.push(`# 阈值：${fmtSize(props.opts?.min_size || 0)} ；统计 ${props.data?.items?.length || 0} 项；导出过滤后 ${list.length} 项`)
    lines.push(`# 提示：把下文复制给 AI，问"哪些能安全清理/分别是什么文件/可否删除"。`)
    lines.push('')
    lines.push('| # | 路径 | 大小 | 修改时间 | 是否系统 |')
    lines.push('|---|------|------|---------|---------|')
    list.forEach((f, i) => {
      lines.push(`| ${i + 1} | ${f.path} | ${humanSize(f.size)} | ${f.modified.replace('T', ' ').slice(0, 19)} | ${f.is_system ? '是' : '否'} |`)
    })
    const text = lines.join('\n')
    const ts = new Date().toISOString().replace(/[:T]/g, '-').slice(0, 19)
    const filename = `大文件清单_${ts}.md`
    const savedPath = await api.saveExport(filename, text)
    emit('toast', `已导出 ${list.length} 项，正在打开所在位置`)
    // 自动弹出资源管理器并选中导出的文件
    try { await api.openInExplorer(savedPath) } catch (e) { /* ignore */ }
  } catch (e) {
    emit('toast', '导出失败：' + e, 'error')
  }
}
</script>

<template>
  <div>
    <div class="summary-cards" v-if="data">
      <div class="sum-card">
        <div class="label">大文件总数</div>
        <div class="value">{{ data.items.length.toLocaleString() }}</div>
      </div>
      <div class="sum-card">
        <div class="label">累计大小</div>
        <div class="value warn">{{ fmtSize(data.total_size) }}</div>
      </div>
      <div class="sum-card">
        <div class="label">扫描用时</div>
        <div class="value">{{ fmtMs(data.duration_ms) }}</div>
      </div>
      <div class="sum-card">
        <div class="label">跳过错误</div>
        <div class="value">{{ data.skipped_errors.toLocaleString() }}</div>
      </div>
    </div>

    <div class="toolbar">
      <button class="btn primary sm" @click="emit('scan')" :disabled="scanning">{{ scanning ? '扫描中...' : '扫描大文件' }}</button>
      <input v-model="keyword" class="search" placeholder="按路径过滤..." />
      <span style="font-size:12px;color:var(--text-3)">共 {{ (data?.items.length || 0).toLocaleString() }} 项</span>
      <button class="btn ghost sm" @click="exportList" :disabled="!filtered.length">导出列表</button>
    </div>

    <div v-if="selected.size" class="batchbar">
      <span style="font-size:13px;color:var(--primary);font-weight:600">已选 {{ selected.size }} 项 / {{ fmtSize(totalSelected) }}</span>
      <div style="flex:1;display:flex;gap:6px">
        <button class="btn ghost sm" @click="selectAll">全选当前</button>
        <button class="btn ghost sm" @click="clearSel">取消选择</button>
        <button class="btn danger sm" @click="trashSelected">删除到回收站</button>
      </div>
    </div>

    <div v-if="!data" class="empty">
      <p class="empty-title">尚未扫描</p>
      <p class="empty-desc">点击上方"扫描大文件"按钮，列出磁盘上超过阈值的文件</p>
    </div>

    <div v-else-if="!filtered.length" class="empty">
      <p class="empty-title">没有匹配的大文件</p>
    </div>

    <div v-else class="table">
      <div class="row head" style="grid-template-columns: 36px 1fr 110px 150px 100px">
        <span></span><span>文件路径</span><span>大小</span><span>修改时间</span><span>操作</span>
      </div>
      <div
        v-for="f in filtered.slice(0, 2000)"
        :key="f.path"
        :class="['row', { system: f.is_system }]"
        style="grid-template-columns: 36px 1fr 110px 150px 100px"
      >
        <span><input type="checkbox" :checked="selected.has(f.path)" @change="toggle(f.path, $event.target.checked)" /></span>
        <span class="mono" :title="f.path">📄 {{ shortName(f.path) }}<div style="font-size:11px;color:var(--text-3)">{{ dirOf(f.path) }}</div></span>
        <span style="color:var(--primary);font-weight:600">{{ fmtSize(f.size) }}</span>
        <span style="color:var(--text-2);font-size:12px">{{ f.modified.replace('T', ' ').slice(0, 19) }}</span>
        <span>
          <button class="btn ghost sm" @click="openInExplorer(f.path)">定位</button>
        </span>
      </div>
    </div>
    <div v-if="filtered.length > 2000" style="text-align:center;font-size:12px;color:var(--text-3);margin-top:8px">
      仅显示前 2000 项，请用上方搜索框过滤
    </div>
  </div>
</template>