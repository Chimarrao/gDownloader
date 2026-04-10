<template>
  <div>
    <!-- Toolbar -->
    <div
      style="
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 14px;
      "
    >
      <span style="font-size: 13px; color: var(--text-secondary)">
        {{ history.length }} download(s) no histórico
      </span>
      <div style="display: flex; gap: 8px; align-items: center">
        <InputText
          v-model="search"
          placeholder="Buscar..."
          size="small"
          style="width: 180px; font-size: 12px"
        />
        <Button
          v-if="history.length > 0"
          label="Limpar histórico"
          icon="pi pi-trash"
          severity="danger"
          size="small"
          text
          @click="handleClear"
        />
      </div>
    </div>

    <!-- Empty state -->
    <div v-if="filtered.length === 0" class="empty-state" style="min-height: 240px">
      <i class="pi pi-history"></i>
      <p>Nenhum download no histórico.<br />Os downloads concluídos aparecerão aqui.</p>
    </div>

    <!-- List -->
    <div v-else style="display: flex; flex-direction: column; gap: 8px">
      <div
        v-for="item in filtered"
        :key="item.id"
        style="
          display: flex;
          align-items: center;
          gap: 12px;
          background: var(--surface-card);
          border: 1px solid var(--surface-border);
          border-radius: 10px;
          padding: 12px 14px;
        "
      >
        <img
          v-if="item.thumbnail"
          :src="item.thumbnail"
          style="
            width: 64px;
            height: 36px;
            border-radius: 6px;
            object-fit: cover;
            flex-shrink: 0;
          "
          alt=""
        />
        <div
          v-else
          style="
            width: 64px;
            height: 36px;
            background: var(--surface-section);
            border-radius: 6px;
            flex-shrink: 0;
          "
        ></div>

        <div style="flex: 1; min-width: 0">
          <div
            style="
              font-size: 13px;
              font-weight: 600;
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            "
          >
            {{ item.title || item.url }}
          </div>
          <div style="font-size: 11px; color: var(--text-secondary); margin-top: 2px">
            {{ formatDate(item.date) }}
            <span v-if="item.formatId" style="margin-left: 8px">· {{ item.formatId }}</span>
          </div>
        </div>

        <div style="display: flex; gap: 4px; flex-shrink: 0">
          <Button
            v-if="item.outputPath"
            v-tooltip.bottom="'Abrir arquivo'"
            icon="pi pi-play-circle"
            severity="success"
            size="small"
            text
            rounded
            @click="openFile(item.outputPath!)"
          />
          <Button
            v-if="item.outputPath"
            v-tooltip.bottom="'Mostrar na pasta'"
            icon="pi pi-folder-open"
            severity="secondary"
            size="small"
            text
            rounded
            @click="showInFolder(item.outputPath!)"
          />
          <Button
            v-tooltip.bottom="'Baixar novamente'"
            icon="pi pi-download"
            severity="info"
            size="small"
            text
            rounded
            @click="$emit('redownload', item.url)"
          />
          <Button
            v-tooltip.bottom="'Remover do histórico'"
            icon="pi pi-times"
            severity="danger"
            size="small"
            text
            rounded
            @click="removeItem(item.id)"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import Tooltip from 'primevue/tooltip'

const vTooltip = Tooltip

interface HistoryItem {
  id: string
  url: string
  title: string
  thumbnail: string
  date: string
  formatId: string
  outputPath?: string
}

const emit = defineEmits<{
  (e: 'redownload', url: string): void
}>()

const history = ref<HistoryItem[]>([])
const search = ref('')

const filtered = computed(() => {
  if (!search.value.trim()) return history.value
  const q = search.value.toLowerCase()
  return history.value.filter(
    (h) => h.title.toLowerCase().includes(q) || h.url.toLowerCase().includes(q)
  )
})

onMounted(async () => {
  try {
    history.value = (await window.api.loadHistory()).reverse()
  } catch {
    history.value = []
  }
})

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString('pt-BR', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    })
  } catch {
    return iso
  }
}

async function openFile(filePath: string): Promise<void> {
  await window.api.openPath(filePath)
}

function showInFolder(filePath: string): void {
  window.api.showInFolder(filePath)
}

function removeItem(id: string): void {
  history.value = history.value.filter((h) => h.id !== id)
  window.api.saveHistory([...history.value].reverse())
}

async function handleClear(): Promise<void> {
  history.value = []
  await window.api.clearHistory()
}

/** Expõe método para App.vue adicionar item ao histórico */
function addToHistory(item: HistoryItem): void {
  history.value.unshift(item)
  window.api.saveHistory([...history.value].reverse())
}

defineExpose({ addToHistory })
</script>
