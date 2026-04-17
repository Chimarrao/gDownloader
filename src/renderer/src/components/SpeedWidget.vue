<template>
  <div class="speed-widget">
    <svg class="sparkline" :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" preserveAspectRatio="none">
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
      <span class="speed-up">↑ —</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const WIDTH = 120
const HEIGHT = 36

const props = defineProps<{
  speedHistory: number[]
  currentSpeed: number
  lineColor?: string
}>()

const lineColor = computed(() => props.lineColor ?? 'var(--accent-color)')

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
  const bps = props.currentSpeed
  if (!bps || bps <= 0) return '0 KB/s'
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(0)} KB/s`
  return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
})
</script>

<style scoped>
.speed-widget {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
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
  gap: 2px;
  font-size: 11px;
  line-height: 1.2;
}

.speed-down {
  color: var(--accent-color);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.speed-up {
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}
</style>
