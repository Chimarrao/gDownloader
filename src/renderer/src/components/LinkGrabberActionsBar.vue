<template>
  <div class="actions-row">
    <button
      class="btn-clear"
      :disabled="disableClear"
      :title="t('linkGrabberClearTitle')"
      @click="$emit('clear')"
    >
      <i class="pi pi-trash"></i>
      {{ t('clear') }}
    </button>
    <button
      class="btn-add"
      :disabled="disableAdd"
      :class="{ loading: adding }"
      :title="addTitle"
      @click="$emit('add')"
    >
      <i :class="adding ? 'pi pi-spin pi-spinner' : 'pi pi-plus'"></i>
      {{ addButtonLabel }}
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

import { useI18n } from '../i18n'

const props = defineProps<{
  disableClear: boolean
  disableAdd: boolean
  adding: boolean
  selectedCount: number
  addButtonLabel: string
}>()

defineEmits<{
  (e: 'clear'): void
  (e: 'add'): void
}>()

const { t } = useI18n()

const addTitle = computed(() => {
  if (props.adding) {
    return t('linkGrabberAddingSelectedTitle')
  }
  return `${t('linkGrabberAddTitle')} ${props.selectedCount} ${t('linkGrabberSelectedSuffix')}`
})
</script>

<style scoped>
.actions-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

.btn-clear,
.btn-add {
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 11px 16px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}

.btn-clear {
  background: transparent;
  color: var(--text-muted);
  border-color: var(--border-color);
}

.btn-add {
  background: linear-gradient(90deg, #5b7cff, #6f63ff);
  border-color: transparent;
  color: #fff;
  flex: 1;
  box-shadow: 0 2px 12px rgba(99, 102, 241, 0.35);
}

.btn-add:not(:disabled):hover {
  box-shadow: 0 4px 20px rgba(99, 102, 241, 0.5);
  transform: translateY(-1px);
  transition: all 0.15s ease;
}

.btn-clear:disabled,
.btn-add:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
</style>
