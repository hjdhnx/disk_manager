<script setup>
import { ref, onMounted, onUnmounted, computed, provide } from 'vue'
import OverviewPanel from './components/OverviewPanel.vue'
import BigFilesPanel from './components/BigFilesPanel.vue'
import DuplicatePanel from './components/DuplicatePanel.vue'
import HistoryPanel from './components/HistoryPanel.vue'
import * as api from './api.js'

const tabs = [
  { key: 'overview', name: '磁盘概览', icon: '◐' },
  { key: 'bigfiles', name: '大文件', icon: '▤' },
  { key: 'duplicates', name: '重复文件', icon: '◳' },
  { key: 'history', name: '清理记录', icon: '⌚' },
]
const activeTab = ref('overview')

const disks = ref([])
const currentDisk = ref('')
const currentDiskInfo = computed(() => disks.value.find(d => d.mount === currentDisk.value))
const currentRoot = computed(() => currentDisk.value || 'C:\\')

// 进度
const progress = ref(null) // { stage, current, scanned, hashed, bytes, elapsed_ms }
const scanning = ref(false)

// 通用配置
const minSizeMB = ref(100)
const opts = computed(() => ({
  min_size: minSizeMB.value * 1024 * 1024,
  skip_system: true,
  include_hidden: true,
  top: 40,
  with_big_files: true,
}))

// 共享的数据
const summary = ref(null)
const bigFiles = ref(null)
const dupResult = ref(null)

// toast
const toast = ref(null)
let toastTimer = null
function showToast(msg, type = 'info') {
  toast.value = { msg, type }
  clearTimeout(toastTimer)
  toastTimer = setTimeout(() => (toast.value = null), 3200)
}

async function loadDisks() {
  try {
    disks.value = await api.listDisks()
    if (!currentDisk.value) {
      const sys = disks.value.find(d => d.is_system)
      currentDisk.value = sys ? sys.mount : (disks.value[0]?.mount || 'C:\\')
    }
  } catch (e) {
    showToast('读取磁盘信息失败：' + e, 'error')
  }
}

function selectDisk(m) {
  currentDisk.value = m
  // 重置数据
  summary.value = null
  bigFiles.value = null
  dupResult.value = null
}

async function doScan(type) {
  if (scanning.value) return
  scanning.value = true
  progress.value = { stage: 'scanning', current: '', scanned: 0, hashed: 0, bytes: 0, elapsed_ms: 0 }
  try {
    if (type === 'overview') {
      summary.value = await api.scanSpace(currentRoot.value, opts.value)
    } else if (type === 'bigfiles') {
      bigFiles.value = await api.scanBigFiles(currentRoot.value, opts.value)
    } else if (type === 'duplicates') {
      dupResult.value = await api.findDuplicates(currentRoot.value, opts.value)
    }
  } catch (e) {
    showToast('扫描失败：' + e, 'error')
  } finally {
    scanning.value = false
    progress.value = null
  }
}

function onScanProgress(payload) {
  progress.value = payload
}

async function doRefreshDuplicates() {
  if (!dupResult.value) return
  scanning.value = true
  try {
    const r = await api.refreshDuplicates(dupResult.value)
    dupResult.value = r
    const removed = (r.groups || []).length
    showToast(`已刷新，剩余 ${removed} 组`)
  } catch (e) {
    showToast('刷新失败：' + e, 'error')
  } finally {
    scanning.value = false
    progress.value = null
  }
}

let unlistenList = []
onMounted(async () => {
  unlistenList.push(await api.onScanProgress(onScanProgress))
  await loadDisks()
})
onUnmounted(() => {
  unlistenList.forEach(fn => fn && fn())
  clearTimeout(toastTimer)
})

function onChildToast(msg, type) {
  showToast(msg, type)
}
async function onChildRefreshed() {
  await loadDisks()
}

function fmtSize(n) {
  if (!n) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0, v = n
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${u[i]}`
}
function fmtMs(ms) {
  if (ms < 1000) return `${ms} ms`
  return `${(ms / 1000).toFixed(1)} s`
}
provide('fmtSize', fmtSize)
provide('fmtMs', fmtMs)
</script>

<template>
  <div class="app">
    <header class="app-header">
      <div class="brand">
        <div class="logo">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <ellipse cx="12" cy="5" rx="8" ry="3" />
            <path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5" />
            <path d="M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" />
            <path d="M9.5 12l2-2 2 2" stroke-width="2.2" />
          </svg>
        </div>
        <div>
          <h1>磁盘清理帮手</h1>
          <p class="subtitle">扫一扫 · 看一看 · 清一清</p>
        </div>
      </div>
      <div class="header-actions">
        <label style="font-size:12px;color:var(--text-3)">大文件阈值:</label>
        <input v-model.number="minSizeMB" type="number" min="1" step="10"
          style="width:72px;padding:7px 8px;border:1px solid var(--border);border-radius:8px;background:var(--card);color:var(--text);font-size:13px" />
        <label style="font-size:12px;color:var(--text-3)">MB</label>
        <select v-model="currentDisk" @change="selectDisk(currentDisk)" class="disk-sel">
          <option v-for="d in disks" :key="d.mount" :value="d.mount">
            {{ d.mount }} ({{ (d.used / 1024 / 1024 / 1024).toFixed(0) }}GB / {{ (d.total / 1024 / 1024 / 1024).toFixed(0) }}GB){{ d.is_system ? ' 系统' : '' }}
          </option>
        </select>
      </div>
    </header>

    <div class="layout">
      <aside class="sidebar">
        <div v-for="t in tabs" :key="t.key"
          :class="['tab-item', { active: activeTab === t.key }]"
          @click="activeTab = t.key">
          <span class="icon">{{ t.icon }}</span>
          <span>{{ t.name }}</span>
        </div>
      </aside>

      <div class="content">
        <OverviewPanel
          v-if="activeTab === 'overview'"
          :root="currentRoot"
          :summary="summary"
          :scanning="scanning"
          :currentDisk="currentDiskInfo"
          :opts="opts"
          :data="summary"
          @scan="doScan('overview')"
          @toast="onChildToast"
          @refresh-disks="onChildRefreshed"
        />
        <BigFilesPanel
          v-if="activeTab === 'bigfiles'"
          :root="currentRoot"
          :data="bigFiles"
          :scanning="scanning"
          :opts="opts"
          @scan="doScan('bigfiles')"
          @toast="onChildToast"
        />
        <DuplicatePanel
          v-if="activeTab === 'duplicates'"
          :root="currentRoot"
          :data="dupResult"
          :scanning="scanning"
          @scan="doScan('duplicates')"
          @refresh="doRefreshDuplicates"
          @toast="onChildToast"
        />
        <HistoryPanel
          v-if="activeTab === 'history'"
          @toast="onChildToast"
        />
      </div>
    </div>

    <div v-if="scanning" class="progress">
      <span class="label">扫描中</span>
      <div class="pbar"><div></div></div>
      <span class="stat">{{ progress?.stage === 'hashing' ? `hash ${progress.hashed}` : `${(progress?.scanned||0).toLocaleString()} 项` }}</span>
    </div>

    <Transition name="toast">
      <div v-if="toast" :class="['toast', toast.type]">{{ toast.msg }}</div>
    </Transition>
  </div>
</template>