<template>
  <div class="error-state">
    <div class="error-icon">
      <i :class="props.icon"></i>
    </div>
    <div class="error-body">
      <div class="error-title">{{ props.title }}</div>
      <div v-if="props.description" class="error-description">{{ props.description }}</div>
      <div v-if="props.actions.length" class="error-actions">
        <button
          v-for="action in props.actions"
          :key="action.label"
          class="error-btn"
          :class="action.variant || 'secondary'"
          @click="action.handler()"
        >
          <i v-if="action.icon" :class="action.icon"></i>
          {{ action.label }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
interface Action {
  label: string
  icon?: string
  variant?: 'primary' | 'secondary' | 'danger'
  handler: () => void
}

const props = withDefaults(defineProps<{
  title: string
  description?: string
  icon?: string
  actions?: Action[]
}>(), {
  icon: 'pi pi-exclamation-triangle',
  actions: () => []
})
</script>

<style scoped>
.error-state {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 10px;
  background: color-mix(in srgb, var(--bg-card) 80%, #ef444408);
  border: 1px solid color-mix(in srgb, var(--border-color) 60%, #ef444420);
}

.error-icon {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border-radius: 7px;
  background: rgba(239, 68, 68, 0.12);
  border: 1px solid rgba(239, 68, 68, 0.2);
  display: flex;
  align-items: center;
  justify-content: center;
  color: #ef4444;
  font-size: 12px;
  margin-top: 1px;
}

.error-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.error-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
}

.error-description {
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.error-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
  flex-wrap: wrap;
}

.error-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px;
  border-radius: 7px;
  border: 1px solid var(--border-color);
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s ease;
}

.error-btn.secondary {
  background: transparent;
  color: var(--text-muted);
}

.error-btn.secondary:hover {
  background: rgba(99, 102, 241, 0.12);
  border-color: var(--accent-color);
  color: var(--accent-color);
}

.error-btn.primary {
  background: rgba(124, 111, 255, 0.15);
  color: var(--accent-color);
  border-color: color-mix(in srgb, var(--accent-color) 40%, var(--border-color));
}

.error-btn.primary:hover {
  background: rgba(124, 111, 255, 0.25);
}

.error-btn.danger {
  background: transparent;
  color: #ef4444;
  border-color: rgba(239, 68, 68, 0.3);
}

.error-btn.danger:hover {
  background: rgba(239, 68, 68, 0.12);
  border-color: #ef4444;
}
</style>
