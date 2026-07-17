<script setup>
import { ref, inject, onMounted } from 'vue'
import * as api from '../api.js'

const emit = defineEmits(['toast'])
const fmtSize = inject('fmtSize')

const entries = ref([])
const loading = ref(false)

async function refresh() {
  loading.value = true
  try {
    entries.value = await api.getCleanHistory()
  } catch (e) {
    emit('toast', '读取记录失败：' + e, 'error')
  } finally {
    loading.value = false
  }
}

async function clearAll() {
  if (!confirm('确认清空所有清理记录？（仅清记录，不影响磁盘）')) return
  try {
    await api.clearCleanHistory()
    entries.value = []
    emit('toast', '记录已清空')
  } catch (e) {
    emit('toast', '清空失败：' + e, 'error')
  }
}

onMounted(refresh)
</script>

<template>
  <div>
    <div class="toolbar">
      <button class="btn ghost sm" @click="refresh">刷新</button>
      <button class="btn danger sm" @click="clearAll" :disabled="!entries.length">清空记录</button>
      <span style="font-size:12px;color:var(--text-3)">共 {{ entries.length }} 次清理</span>
    </div>

    <div v-if="loading && !entries.length" class="empty">加载中...</div>

    <div v-else-if="!entries.length" class="empty">
      <p class="empty-title">还没有清理记录</p>
      <p class="empty-desc">在"大文件"或"重复文件"中删除文件后，这里会留底。</p>
    </div>

    <div v-else>
      <div v-for="(h, i) in entries" :key="i" class="h-item">
        <div class="h-left">
          <div>
            <span class="h-time">{{ h.time.replace('T', ' ').slice(0, 19) }}</span>
            <span :class="['h-action', h.action]">{{ h.action === 'trash' ? '回收站' : '永久删除' }}</span>
          </div>
          <div class="h-files">
            <div v-for="(it, j) in h.items.slice(0, 5)" :key="j" class="mono" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
              · {{ it.path }}
            </div>
            <div v-if="h.items.length > 5" style="color:var(--text-3)">...还有 {{ h.items.length - 5 }} 项</div>
          </div>
        </div>
        <div class="h-freed">+{{ fmtSize(h.freed_bytes) }}</div>
      </div>
    </div>
  </div>
</template>