<template>
  <div class="link-input-panel">
    <div class="grabber-header">
      <h2 class="grabber-title">{{ t('linkGrabberTitle') }}</h2>
      <p class="grabber-sub">{{ t('linkGrabberSub') }}</p>
    </div>

    <div class="url-field">
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

const props = defineProps<{
  modelValue: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
  (e: 'resize', event: Event): void
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
