<template>
  <div class="speed-widget" :title="hostBreakdownTitle" @click="expanded = !expanded">
    <svg class="speed-gauge" viewBox="0 0 64 40" aria-hidden="true">
      <path class="gauge-track" d="M10 34 A22 22 0 0 1 54 34" />
      <path class="gauge-fill" d="M10 34 A22 22 0 0 1 54 34" :style="{ strokeDashoffset: gaugeOffset }" />
      <text x="32" y="30" text-anchor="middle">{{ gaugePercent }}%</text>
    </svg>
    <svg v-if="expanded" class="sparkline" :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" preserveAspectRatio="none">
      <polyline
        v-if="points.length > 1"
        :points="points"
        fill="none"
        :stroke="lineColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        vector-effect="non-scaling-stroke"
      />
      <polyline
        v-if="points.length > 1"
        :points="fillPoints"
        fill="url(#sparkGrad)"
        stroke="none"
        vector-effect="non-scaling-stroke"
      />
      <defs>
        <linearGradient id="sparkGrad" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" :stop-color="lineColor" stop-opacity="0.3" />
          <stop offset="100%" :stop-color="lineColor" stop-opacity="0" />
        </linearGradient>
      </defs>
    </svg>
    <div class="speed-labels">
      <span class="speed-down">↓ {{ formattedSpeed }}</span>
      <span v-if="topHost" class="speed-host">{{ topHost }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatSpeed } from '../utils/format'

const WIDTH = 120
const HEIGHT = 36

const props = defineProps<{
  speedHistory: number[]
  currentSpeed: number
  perHostSpeed?: Record<string, number>
  lineColor?: string
}>()

const lineColor = computed(() => props.lineColor ?? 'var(--accent-color)')
const expanded = ref(false)
const gaugeLength = 69.1

const points = computed(() => {
  const data = props.speedHistory
  if (data.length < 2) return ''
  const max = Math.max(...data, 1)
  return data
    .map((v, i) => {
      const x = (i / (data.length - 1)) * WIDTH
      const y = HEIGHT - (v / max) * HEIGHT * 0.85
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
})

const fillPoints = computed(() => {
  const data = props.speedHistory
  if (data.length < 2) return ''
  const max = Math.max(...data, 1)
  const pts = data.map((v, i) => {
    const x = (i / (data.length - 1)) * WIDTH
    const y = HEIGHT - (v / max) * HEIGHT * 0.85
    return `${x.toFixed(1)},${y.toFixed(1)}`
  })
  pts.push(`${WIDTH},${HEIGHT}`)
  pts.unshift(`0,${HEIGHT}`)
  return pts.join(' ')
})

const formattedSpeed = computed(() => {
  return formatSpeed(props.currentSpeed)
})

const maxReference = computed(() => Math.max(...props.speedHistory, props.currentSpeed, 1))
const gaugeRatio = computed(() => Math.min(1, props.currentSpeed / maxReference.value))
const gaugeOffset = computed(() => `${gaugeLength * (1 - gaugeRatio.value)}`)
const gaugePercent = computed(() => Math.round(gaugeRatio.value * 100))

const hostEntries = computed(() =>
  Object.entries(props.perHostSpeed ?? {})
    .filter(([, speed]) => speed > 0)
    .sort((a, b) => b[1] - a[1])
)

const topHost = computed(() => {
  const top = hostEntries.value[0]
  return top ? `${top[0]} ${formatSpeed(top[1])}` : ''
})

const hostBreakdownTitle = computed(() => {
  if (hostEntries.value.length === 0) return 'Sem tráfego por host'
  return hostEntries.value.map(([host, speed]) => `${host}: ${formatSpeed(speed)}`).join('\n')
})
</script>

<style scoped>
.speed-widget {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  cursor: pointer;
}

.speed-gauge {
  width: 70px;
  height: 42px;
  overflow: visible;
}

.gauge-track,
.gauge-fill {
  fill: none;
  stroke-width: 5;
  stroke-linecap: round;
}

.gauge-track {
  stroke: color-mix(in srgb, var(--text-muted) 20%, transparent);
}

.gauge-fill {
  stroke: var(--accent-color);
  stroke-dasharray: 69.1;
  transition: stroke-dashoffset 0.25s ease;
}

.speed-gauge text {
  fill: var(--text-muted);
  font-size: 9px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.sparkline {
  width: 120px;
  height: 36px;
  border-radius: 4px;
  overflow: visible;
}

.speed-labels {
  display: flex;
  flex-direction: column;
  font-size: 11px;
  line-height: 1.2;
}

.speed-down {
  color: var(--accent-color);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.speed-host {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
  font-size: 10px;
}
</style>
