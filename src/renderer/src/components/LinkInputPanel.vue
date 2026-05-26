<template>
  <div class="link-input-panel">
    <div class="grabber-header">
      <h2 class="grabber-title">{{ t('linkGrabberTitle') }}</h2>
      <p class="grabber-sub">{{ t('linkGrabberSub') }}</p>
    </div>

    <div class="url-field" data-tour="link-input">
      <label class="field-label">{{ t('linkGrabberInputLabel') }}</label>
      <div class="textarea-wrapper">
        <i class="pi pi-link textarea-icon"></i>
        <textarea
          :value="modelValue"
          class="url-textarea"
          :placeholder="placeholder"
          rows="5"
          @input="onInput"
        ></textarea>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

import { useI18n } from '../i18n'

defineProps<{
  modelValue: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
  (e: 'resize', event: Event): void
  (e: 'imported-links', urls: string[]): void
  (e: 'imported-hashes', hashes: Array<{ filename: string; value: string }>): void
  (e: 'import-error', message: string): void
}>()

const { t } = useI18n()

const placeholder = computed(() => [
  'https://mega.nz/file/...',
  'https://www.mediafire.com/folder/...',
  'https://pixeldrain.com/l/...',
].join('\n'))
function onInput(event: Event): void {
  const value = (event.target as HTMLTextAreaElement).value
  emit('update:modelValue', value)
  emit('resize', event)
}
</script>

<style scoped>
.link-input-panel {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.grabber-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.grabber-title {
  margin: 0;
  font-size: 26px;
  line-height: 1.1;
  color: var(--text-primary);
}

.grabber-sub {
  margin: 0;
  font-size: 13px;
  color: var(--text-secondary);
}

.url-field {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.container-drop {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px;
  border: 1px dashed var(--border-color);
  border-radius: 12px;
  background: color-mix(in srgb, var(--bg-card) 86%, var(--accent-color) 14%);
  transition: border-color 0.15s ease, background 0.15s ease, transform 0.15s ease;
}

.container-drop.is-dragging {
  border-color: var(--accent-color);
  background: color-mix(in srgb, var(--bg-card) 74%, var(--accent-color) 26%);
}

.container-drop.is-importing {
  opacity: 0.82;
}

.container-file-input {
  display: none;
}

.container-button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 36px;
  padding: 0 12px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}

.container-button:disabled {
  cursor: wait;
  opacity: 0.7;
}

.container-hint {
  min-width: 0;
  color: var(--text-secondary);
  font-size: 12px;
}

.field-label {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-muted);
}

.textarea-wrapper {
  position: relative;
}

.textarea-icon {
  position: absolute;
  top: 14px;
  left: 14px;
  font-size: 14px;
  color: var(--text-muted);
}

.url-textarea {
  width: 100%;
  min-height: 140px;
  padding: 14px 16px 14px 38px;
  resize: vertical;
  border-radius: 18px;
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  box-shadow: var(--shadow-sm);
}
</style>
