<script setup>
import { ref, inject, computed } from 'vue'
import * as api from '../api.js'

const props = defineProps({
  root: String,
  data: Object,
  scanning: Boolean
})
const emit = defineEmits(['scan', 'refresh', 'toast'])

const fmtSize = inject('fmtSize')
const fmtMs = inject('fmtMs')

const keyword = ref('')
// 每组选中的待删文件路径集合（不含 keep_index）
const selected = ref(new Set())

const filtered = computed(() => {
  const k = keyword.value.trim().toLowerCase()
  if (!k) return props.data?.groups || []
  return (props.data?.groups || []).filter(g =>
    g.md5.toLowerCase().includes(k) ||
    g.files.some(f => f.path.toLowerCase().includes(k))
  )
})

const totalWasteSelected = computed(() => {
  let s = 0
  for (const p of selected.value) s += sizeOf(p)
  return s
})

function sizeOf(path) {
  for (const g of props.data?.groups || []) {
    for (const f of g.files) if (f.path === path) return f.size
  }
  return 0
}

function toggle(p, checked) {
  const n = new Set(selected.value)
  if (checked) n.add(p); else n.delete(p)
  selected.value = n
}

function selectAllDup() {
  const n = new Set(selected.value)
  for (const g of filtered.value) {
    g.files.forEach((f, i) => { if (i !== g.keep_index) n.add(f.path) })
  }
  selected.value = n
}

function clearSel() { selected.value = new Set() }

async function trashSelected() {
  const paths = Array.from(selected.value)
  if (!paths.length) return
  if (!confirm(`确认将选中的 ${paths.length} 个重复文件（共 ${fmtSize(totalWasteSelected.value)}）删除到回收站？\n建议保留每组标"保留"的那个文件。`)) return
  try {
    const r = await api.trashFiles(paths)
    emit('toast', `已清理 ${r.succeeded.length} 个，释放 ${fmtSize(r.freed_bytes)}`)
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
</script>

<template>
  <div>
    <div class="summary-cards" v-if="data">
      <div class="sum-card">
        <div class="label">重复组数</div>
        <div class="value">{{ data.groups.length.toLocaleString() }}</div>
      </div>
      <div class="sum-card">
        <div class="label">可回收空间</div>
        <div class="value warn">{{ fmtSize(data.total_waste) }}</div>
      </div>
      <div class="sum-card">
        <div class="label">参与 hash 文件</div>
        <div class="value">{{ data.hashed_files.toLocaleString() }}</div>
      </div>
      <div class="sum-card">
        <div class="label">用时</div>
        <div class="value">{{ fmtMs(data.duration_ms) }}</div>
      </div>
    </div>

    <div class="toolbar">
      <button class="btn primary sm" @click="emit('scan')" :disabled="scanning">{{ scanning ? '扫描中...' : '扫描重复文件' }}</button>
      <button v-if="data?.groups?.length" class="btn ghost sm" @click="emit('refresh')" :disabled="scanning">刷新状态</button>
      <input v-model="keyword" class="search" placeholder="按 MD5 或路径过滤..." />
      <span style="font-size:12px;color:var(--text-3)">共 {{ (data?.groups.length || 0).toLocaleString() }} 组</span>
      <button v-if="data?.groups?.length" class="btn ghost sm" @click="selectAllDup">全选所有副本</button>
    </div>
    <div v-if="data?.groups?.length" style="font-size:12px;color:var(--text-3);margin:-6px 0 10px">
      💡 删除文件后点"刷新状态"快速过滤掉已不存在的副本，无需重算 MD5。
    </div>

    <div v-if="selected.size" class="batchbar">
      <span style="font-size:13px;color:var(--primary);font-weight:600">已选 {{ selected.size }} 项 / {{ fmtSize(totalWasteSelected) }}</span>
      <div style="flex:1;display:flex;gap:6px">
        <button class="btn ghost sm" @click="clearSel">取消</button>
        <button class="btn danger sm" @click="trashSelected">删除到回收站</button>
      </div>
    </div>

    <div v-if="!data" class="empty">
      <p class="empty-title">尚未扫描</p>
      <p class="empty-desc">点击"扫描重复文件"，按 MD5 识别磁盘上的重复副本</p>
    </div>

    <div v-else-if="!filtered.length" class="empty">
      <p class="empty-title">没有发现重复文件</p>
      <p class="empty-desc">太干净了，磁盘状况良好。</p>
    </div>

    <template v-else>
      <div v-for="(g, gi) in filtered.slice(0, 500)" :key="g.md5 + gi" class="dup-group">
        <div class="dg-head">
          <div>
            <span class="meta">MD5: {{ g.md5.slice(0, 16) }}…</span>
            <span style="margin-left:12px;font-size:12px;color:var(--text-2)">{{ fmtSize(g.size) }} × {{ g.files.length }} 副本</span>
          </div>
          <div>
            <span class="waste">可回收 {{ fmtSize(g.waste) }}</span>
          </div>
        </div>
        <div
          v-for="(f, i) in g.files"
          :key="f.path"
          :class="['dup-row', { keep: i === g.keep_index }]"
          style="grid-template-columns: 36px 70px 1fr 110px 150px 80px"
        >
          <span>
            <input v-if="i !== g.keep_index" type="checkbox" :checked="selected.has(f.path)" @change="toggle(f.path, $event.target.checked)" />
          </span>
          <span>
            <span :class="['dup-flag', i === g.keep_index ? 'keep' : 'dup']">
              {{ i === g.keep_index ? '保留' : '副本' }}
            </span>
          </span>
          <span class="mono" :title="f.path">
            {{ shortName(f.path) }}
            <div style="font-size:11px;color:var(--text-3)">{{ dirOf(f.path) }}</div>
          </span>
          <span style="color:var(--primary)">{{ fmtSize(f.size) }}</span>
          <span style="color:var(--text-2);font-size:12px">{{ f.modified.replace('T', ' ').slice(0, 19) }}</span>
          <span><button class="btn ghost sm" @click="openInExplorer(f.path)">定位</button></span>
        </div>
      </div>
      <div v-if="filtered.length > 500" style="text-align:center;font-size:12px;color:var(--text-3);margin-top:8px">
        仅显示前 500 组，请用上方搜索框过滤
      </div>
    </template>
  </div>
</template>