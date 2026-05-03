<template>
  <div class="link-input-panel">
    <div class="grabber-header">
      <h2 class="grabber-title">{{ t('linkGrabberTitle') }}</h2>
      <p class="grabber-sub">{{ t('linkGrabberSub') }}</p>
    </div>

    <div
      class="container-drop"
      :class="{ 'is-dragging': dragging, 'is-importing': importing }"
      @dragenter.prevent="onDragEnter"
      @dragover.prevent="onDragEnter"
      @dragleave.prevent="onDragLeave"
      @drop.prevent="onDrop"
    >
      <input
        ref="fileInput"
        class="container-file-input"
        type="file"
        accept=".dlc,.ccf,.rsdf,.sfv"
        @change="onFileSelected"
      >
      <button
        class="container-button"
        type="button"
        :disabled="importing"
        @click="fileInput?.click()"
      >
        <i :class="importing ? 'pi pi-spin pi-spinner' : 'pi pi-upload'"></i>
        <span>{{ importing ? t('linkGrabberContainerImporting') : t('linkGrabberContainerDrop') }}</span>
      </button>
      <span class="container-hint">{{ t('linkGrabberContainerHint') }}</span>
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
import { computed, ref } from 'vue'

import { useI18n } from '../i18n'

const props = defineProps<{
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
const dragging = ref(false)
const importing = ref(false)
const fileInput = ref<HTMLInputElement | null>(null)

function onInput(event: Event): void {
  const value = (event.target as HTMLTextAreaElement).value
  emit('update:modelValue', value)
  emit('resize', event)
}

function isContainerFile(file: File): boolean {
  return /\.(dlc|ccf|rsdf)$/i.test(file.name)
}

function isSfvFile(file: File): boolean {
  return /\.sfv$/i.test(file.name)
}

function onDragEnter(): void {
  dragging.value = true
}

function onDragLeave(event: DragEvent): void {
  const current = event.currentTarget as HTMLElement | null
  const related = event.relatedTarget as Node | null
  if (!current || !related || !current.contains(related)) {
    dragging.value = false
  }
}

async function importContainer(file: File): Promise<void> {
  if (isSfvFile(file)) {
    const text = await file.text()
    const hashes = parseSfv(text)
    if (hashes.length === 0) {
      emit('import-error', 'Nenhum CRC32 válido encontrado no .sfv')
      return
    }
    emit('imported-hashes', hashes)
    return
  }

  if (!isContainerFile(file)) {
    emit('import-error', t('linkGrabberContainerUnsupported'))
    return
  }

  importing.value = true
  try {
    const links = await window.api.links.importContainer(file)
    const urls = links.map((item) => item.url).filter(Boolean)
    if (urls.length === 0) {
      emit('import-error', t('linkGrabberContainerEmpty'))
      return
    }
    emit('imported-links', urls)
  } catch (error) {
    emit('import-error', error instanceof Error ? error.message : t('linkGrabberContainerFailed'))
  } finally {
    importing.value = false
    dragging.value = false
    if (fileInput.value) {
      fileInput.value.value = ''
    }
  }
}

function onDrop(event: DragEvent): void {
  const file = Array.from(event.dataTransfer?.files ?? []).find((item) => isContainerFile(item) || isSfvFile(item))
  dragging.value = false
  if (!file) {
    emit('import-error', t('linkGrabberContainerUnsupported'))
    return
  }
  void importContainer(file)
}

function parseSfv(text: string): Array<{ filename: string; value: string }> {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith(';'))
    .map((line) => {
      const match = line.match(/^(.+?)\s+([a-fA-F0-9]{8})$/)
      if (!match) return null
      return {
        filename: match[1].trim(),
        value: match[2].toLowerCase(),
      }
    })
    .filter((item): item is { filename: string; value: string } => item !== null)
}

function onFileSelected(event: Event): void {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) {
    return
  }
  void importContainer(file)
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
