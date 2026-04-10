<template>
  <div>
    <!-- Cards de resumo -->
    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-bottom: 20px">
      <div class="stat-card">
        <div class="stat-value">{{ stats.total }}</div>
        <div class="stat-label">Downloads totais</div>
      </div>
      <div class="stat-card">
        <div class="stat-value">{{ stats.last7Days }}</div>
        <div class="stat-label">Últimos 7 dias</div>
      </div>
      <div class="stat-card">
        <div class="stat-value">{{ stats.formatsCount }}</div>
        <div class="stat-label">Formatos distintos</div>
      </div>
    </div>

    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 16px">
      <!-- Downloads por dia (últimos 7 dias) -->
      <div class="stat-section">
        <h3 class="stat-section-title">
          <i class="pi pi-calendar" style="margin-right: 6px"></i>Últimos 7 dias
        </h3>
        <div v-if="stats.total === 0" class="stat-empty">Sem dados ainda</div>
        <div v-else style="display: flex; flex-direction: column; gap: 8px">
          <div v-for="day in stats.dailySeries" :key="day.label" style="display: flex; align-items: center; gap: 10px">
            <span style="font-size: 11px; color: var(--text-secondary); min-width: 36px; text-align: right">
              {{ day.label }}
            </span>
            <div style="flex: 1; height: 18px; background: var(--surface-section); border-radius: 4px; overflow: hidden">
              <div
                :style="{
                  height: '100%',
                  width: day.pct + '%',
                  background: 'var(--accent-gradient)',
                  borderRadius: '4px',
                  transition: 'width 0.4s ease',
                  minWidth: day.count > 0 ? '4px' : '0'
                }"
              ></div>
            </div>
            <span style="font-size: 11px; color: var(--text-secondary); min-width: 20px; text-align: right">
              {{ day.count }}
            </span>
          </div>
        </div>
      </div>

      <!-- Top formatos -->
      <div class="stat-section">
        <h3 class="stat-section-title">
          <i class="pi pi-list" style="margin-right: 6px"></i>Formatos mais usados
        </h3>
        <div v-if="stats.topFormats.length === 0" class="stat-empty">Sem dados ainda</div>
        <div v-else style="display: flex; flex-direction: column; gap: 8px">
          <div
            v-for="fmt in stats.topFormats"
            :key="fmt.label"
            style="display: flex; align-items: center; gap: 10px"
          >
            <span
              style="
                font-size: 11px;
                font-family: monospace;
                color: var(--text-primary);
                min-width: 110px;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
              "
              :title="fmt.label"
            >
              {{ fmt.label }}
            </span>
            <div style="flex: 1; height: 18px; background: var(--surface-section); border-radius: 4px; overflow: hidden">
              <div
                :style="{
                  height: '100%',
                  width: fmt.pct + '%',
                  background: 'linear-gradient(90deg, var(--accent-color), #a78bfa)',
                  borderRadius: '4px',
                  transition: 'width 0.4s ease',
                  minWidth: '4px'
                }"
              ></div>
            </div>
            <span style="font-size: 11px; color: var(--text-secondary); min-width: 20px; text-align: right">
              {{ fmt.count }}
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'

interface HistoryItem {
  id: string
  url: string
  title: string
  thumbnail: string
  date: string
  formatId: string
  outputPath?: string
}

interface DayStat {
  label: string
  count: number
  pct: number
}

interface FormatStat {
  label: string
  count: number
  pct: number
}

const history = ref<HistoryItem[]>([])

onMounted(async () => {
  try {
    history.value = await window.api.loadHistory()
  } catch {
    history.value = []
  }
})

const stats = computed(() => {
  const items = history.value
  const total = items.length

  // Últimos 7 dias
  const now = new Date()
  const dailySeries: DayStat[] = []
  let maxDay = 0

  for (let i = 6; i >= 0; i--) {
    const d = new Date(now)
    d.setDate(d.getDate() - i)
    const label = d.toLocaleDateString('pt-BR', { day: '2-digit', month: '2-digit' })
    const dayStr = d.toISOString().slice(0, 10)
    const count = items.filter((it) => it.date.startsWith(dayStr)).length
    if (count > maxDay) maxDay = count
    dailySeries.push({ label, count, pct: 0 })
  }
  for (const d of dailySeries) {
    d.pct = maxDay > 0 ? Math.round((d.count / maxDay) * 100) : 0
  }

  // Top formatos (máx 6)
  const fmtMap: Record<string, number> = {}
  for (const it of items) {
    const key = it.formatId || 'padrão'
    fmtMap[key] = (fmtMap[key] || 0) + 1
  }
  const sorted = Object.entries(fmtMap)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 6)
  const maxFmt = sorted[0]?.[1] || 1
  const topFormats: FormatStat[] = sorted.map(([label, count]) => ({
    label,
    count,
    pct: Math.round((count / maxFmt) * 100)
  }))

  const last7Days = dailySeries.reduce((s, d) => s + d.count, 0)
  const formatsCount = Object.keys(fmtMap).length

  return { total, last7Days, formatsCount, dailySeries, topFormats }
})
</script>

<style scoped>
.stat-card {
  background: var(--surface-card);
  border: 1px solid var(--surface-border);
  border-radius: 12px;
  padding: 16px 20px;
  text-align: center;
}

.stat-value {
  font-size: 32px;
  font-weight: 700;
  background: var(--accent-gradient);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
  line-height: 1.1;
}

.stat-label {
  font-size: 12px;
  color: var(--text-secondary);
  margin-top: 4px;
}

.stat-section {
  background: var(--surface-card);
  border: 1px solid var(--surface-border);
  border-radius: 12px;
  padding: 16px;
}

.stat-section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 14px;
}

.stat-empty {
  font-size: 12px;
  color: var(--text-secondary);
  text-align: center;
  padding: 24px 0;
}
</style>
