<template>
  <div class="stats-view">
    <!-- Summary cards -->
    <div class="stat-cards">
      <div class="stat-card">
        <div class="stat-icon stat-icon-total">
          <i class="pi pi-database"></i>
        </div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.total }}</div>
          <div class="stat-label">Downloads totais</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-icon stat-icon-week">
          <i class="pi pi-calendar"></i>
        </div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.last7Days }}</div>
          <div class="stat-label">Últimos 7 dias</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-icon stat-icon-formats">
          <i class="pi pi-file"></i>
        </div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.formatsCount }}</div>
          <div class="stat-label">Formatos distintos</div>
        </div>
      </div>
    </div>

    <!-- Charts grid -->
    <div class="charts-grid">
      <!-- Daily activity -->
      <div class="chart-panel">
        <h3 class="panel-title">
          <i class="pi pi-chart-bar"></i>
          Atividade — Últimos 7 dias
        </h3>

        <div v-if="stats.total === 0" class="chart-empty">
          <i class="pi pi-chart-bar" style="font-size: 24px; opacity: 0.3;"></i>
          <span>Sem dados ainda</span>
        </div>

        <div v-else class="bar-chart">
          <div
            v-for="day in stats.dailySeries"
            :key="day.label"
            class="bar-row"
          >
            <span class="bar-label">{{ day.label }}</span>
            <div class="bar-track">
              <div
                class="bar-fill"
                :style="{ width: day.pct + '%' }"
              ></div>
            </div>
            <span class="bar-count">{{ day.count }}</span>
          </div>
        </div>
      </div>

      <!-- Top formats -->
      <div class="chart-panel">
        <h3 class="panel-title">
          <i class="pi pi-list"></i>
          Formatos mais usados
        </h3>

        <div v-if="stats.topFormats.length === 0" class="chart-empty">
          <i class="pi pi-list" style="font-size: 24px; opacity: 0.3;"></i>
          <span>Sem dados ainda</span>
        </div>

        <div v-else class="bar-chart">
          <div
            v-for="(fmt, idx) in stats.topFormats"
            :key="fmt.label"
            class="bar-row"
          >
            <span class="bar-label bar-label-fmt" :title="fmt.label">{{ fmt.label }}</span>
            <div class="bar-track">
              <div
                class="bar-fill bar-fill-alt"
                :style="{
                  width: fmt.pct + '%',
                  opacity: 1 - idx * 0.1
                }"
              ></div>
            </div>
            <span class="bar-count">{{ fmt.count }}</span>
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
/* ── Layout ─────────────────────────────────────────────────── */
.stats-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* ── Summary cards ──────────────────────────────────────────── */
.stat-cards {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}

.stat-card {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 16px 18px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  transition: box-shadow 0.2s ease, border-color 0.2s ease;
}

.stat-card:hover {
  border-color: color-mix(in srgb, var(--accent-color) 40%, var(--border-color));
  box-shadow: var(--shadow-card);
}

.stat-icon {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  flex-shrink: 0;
}

.stat-icon-total {
  background: rgba(124, 111, 255, 0.15);
  color: var(--accent-color);
  border: 1px solid rgba(124, 111, 255, 0.2);
}

.stat-icon-week {
  background: rgba(34, 197, 94, 0.12);
  color: #22c55e;
  border: 1px solid rgba(34, 197, 94, 0.2);
}

.stat-icon-formats {
  background: rgba(251, 191, 36, 0.12);
  color: #fbbf24;
  border: 1px solid rgba(251, 191, 36, 0.2);
}

.stat-info {
  flex: 1;
  min-width: 0;
}

.stat-value {
  font-size: 28px;
  font-weight: 800;
  line-height: 1.1;
  background: var(--accent-gradient);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.stat-label {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 3px;
  white-space: nowrap;
}

/* ── Charts grid ────────────────────────────────────────────── */
.charts-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

/* ── Chart panel ────────────────────────────────────────────── */
.chart-panel {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 16px;
}

.panel-title {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin: 0 0 16px;
}

.panel-title i {
  color: var(--accent-color);
  font-size: 12px;
}

/* ── Chart empty ────────────────────────────────────────────── */
.chart-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  min-height: 120px;
  color: var(--text-muted);
  font-size: 12px;
}

/* ── Bar chart ──────────────────────────────────────────────── */
.bar-chart {
  display: flex;
  flex-direction: column;
  gap: 9px;
}

.bar-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.bar-label {
  font-size: 11px;
  color: var(--text-muted);
  min-width: 36px;
  text-align: right;
  white-space: nowrap;
}

.bar-label-fmt {
  min-width: 80px;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: 'Courier New', monospace;
  font-size: 10.5px;
  color: var(--text-primary);
}

.bar-track {
  flex: 1;
  height: 16px;
  background: var(--surface-section);
  border-radius: 4px;
  overflow: hidden;
  border: 1px solid var(--border-color);
}

.bar-fill {
  height: 100%;
  border-radius: 4px;
  background: var(--accent-gradient);
  transition: width 0.5s cubic-bezier(0.4, 0, 0.2, 1);
  min-width: 0;
}

.bar-fill-alt {
  background: linear-gradient(90deg, var(--accent-color), var(--accent-light));
}

.bar-count {
  font-size: 11px;
  color: var(--text-muted);
  min-width: 20px;
  text-align: right;
  font-weight: 600;
}
</style>
