<template>
  <div
    ref="viewportRef"
    class="virtual-rows"
    :style="{ maxHeight }"
    @scroll="onScroll"
  >
    <div :style="{ height: `${topSpacer}px` }"></div>
    <div
      v-for="(item, index) in visibleItems"
      :key="getKey(item, startIndex + index)"
      class="virtual-row-shell"
    >
      <slot :item="item" :index="startIndex + index"></slot>
    </div>
    <div :style="{ height: `${bottomSpacer}px` }"></div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'

const props = withDefaults(defineProps<{
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  items: any[]
  itemHeight?: number
  overscan?: number
  maxHeight?: string
  keyField?: string
}>(), {
  itemHeight: 34,
  overscan: 8,
  maxHeight: '420px',
  keyField: 'key',
})

const viewportRef = ref<HTMLElement | null>(null)
const scrollTop = ref(0)

const viewportHeight = computed(() => viewportRef.value?.clientHeight ?? 420)
const startIndex = computed(() =>
  Math.max(0, Math.floor(scrollTop.value / props.itemHeight) - props.overscan),
)
const endIndex = computed(() =>
  Math.min(
    props.items.length,
    startIndex.value + Math.ceil(viewportHeight.value / props.itemHeight) + props.overscan * 2,
  ),
)
const visibleItems = computed(() => props.items.slice(startIndex.value, endIndex.value))
const topSpacer = computed(() => startIndex.value * props.itemHeight)
const bottomSpacer = computed(() =>
  Math.max(0, (props.items.length - endIndex.value) * props.itemHeight),
)

function onScroll(): void {
  scrollTop.value = viewportRef.value?.scrollTop ?? 0
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function getKey(item: any, fallbackIndex: number): string | number {
  if (item && typeof item === 'object' && props.keyField in item) {
    return String(item[props.keyField] ?? fallbackIndex)
  }
  return fallbackIndex
}
</script>

<style scoped>
.virtual-rows {
  overflow-y: auto;
  min-height: 0;
}
</style>
