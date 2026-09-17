<template>
  <div class="torrent-panel">
    <div class="torrent-add-box">
      <input
        v-model="magnetInput"
        class="torrent-magnet-input"
        placeholder="Cole um link magnet: ou escolha um arquivo .torrent"
        @keyup.enter="addTorrent"
      />
      <button class="btn-secondary" @click="pickTorrentFile">
        Arquivo .torrent
      </button>
      <label
        class="tor-required-check"
        title="Kill switch: este torrent nunca conecta a peers fora do Tor."
      >
        <input v-model="torRequiredInput" type="checkbox" />
        <i class="pi pi-shield"></i>
        <span>Tor</span>
      </label>
      <button
        class="btn-primary"
        :disabled="!canAdd || adding"
        @click="addTorrent"
      >
        {{ adding ? "Adicionando..." : "Adicionar" }}
      </button>
    </div>
    <div v-if="pickedFilePath" class="torrent-picked-file">
      <i class="pi pi-file"></i> {{ pickedFilePath }}
      <button class="link-btn" @click="pickedFilePath = ''">remover</button>
    </div>
    <div v-if="addError" class="torrent-add-error">{{ addError }}</div>

    <div v-if="torrents.length === 0" class="torrent-empty">
      Nenhum torrent adicionado ainda. Cole um magnet link ou escolha um arquivo
      .torrent acima.
    </div>

    <div class="torrent-list">
      <div v-for="t in torrents" :key="t.id" class="torrent-row">
        <div class="torrent-row-main">
          <div class="torrent-row-info">
            <span class="torrent-name" :title="t.name">{{
              t.name || t.source
            }}</span>
            <span class="torrent-meta">
              {{ statusLabel(t) }}
              <span
                v-if="t.torRequired"
                class="torrent-tor-badge"
                title="Kill switch Tor ativo"
                ><i class="pi pi-shield"></i
              ></span>
            </span>
          </div>
          <div class="torrent-row-actions">
            <button
              v-if="t.paused"
              class="icon-btn"
              title="Retomar"
              @click="resume(t)"
            >
              <i class="pi pi-play"></i>
            </button>
            <button v-else class="icon-btn" title="Pausar" @click="pause(t)">
              <i class="pi pi-pause"></i>
            </button>
            <button
              class="icon-btn"
              title="Reverificar dados"
              @click="recheck(t)"
            >
              <i class="pi pi-refresh"></i>
            </button>
            <button class="icon-btn" title="Peers" @click="togglePeers(t)">
              <i class="pi pi-sitemap"></i>
            </button>
            <button class="icon-btn" title="Arquivos" @click="toggleFiles(t)">
              <i class="pi pi-list"></i>
            </button>
            <button
              class="icon-btn danger"
              title="Remover"
              @click="remove(t, false)"
            >
              <i class="pi pi-trash"></i>
            </button>
          </div>
        </div>

        <div class="torrent-progress-bar">
          <div
            class="torrent-progress-fill"
            :style="{ width: `${Math.round((t.progress || 0) * 100)}%` }"
          ></div>
        </div>
        <div class="torrent-stats">
          <span>{{ Math.round((t.progress || 0) * 100) }}%</span>
          <span
            >{{ formatBytes(t.bytesCompleted) }} /
            {{ formatBytes(t.totalBytes) }}</span
          >
          <span>↓ {{ formatSpeed(t.downloadBps) }}</span>
          <span>↑ {{ formatSpeed(t.uploadBps) }}</span>
          <span>{{ t.numPeers }} peers · {{ t.numSeeds }} seeds</span>
        </div>
        <div v-if="t.error" class="torrent-error">{{ t.error }}</div>

        <div
          v-if="expandedFiles.has(t.id) && t.files?.length"
          class="torrent-files"
        >
          <label v-for="f in t.files" :key="f.index" class="torrent-file-row">
            <input
              type="checkbox"
              :checked="f.selected"
              @change="onToggleFile(t, f, $event)"
            />
            <span class="torrent-file-path">{{ f.path }}</span>
            <span class="torrent-file-size"
              >{{ formatBytes(f.bytesCompleted) }} /
              {{ formatBytes(f.size) }}</span
            >
          </label>
        </div>

        <div v-if="expandedPeers.has(t.id)" class="torrent-peers">
          <div
            v-if="(peersByTorrent[t.id]?.length ?? 0) === 0"
            class="torrent-peers-empty"
          >
            Nenhum peer conectado ainda.
          </div>
          <div
            v-for="p in peersByTorrent[t.id]"
            :key="p.address"
            class="torrent-peer-row"
          >
            <span class="torrent-peer-addr">{{ p.address }}</span>
            <span class="torrent-peer-client">{{ p.clientName || "?" }}</span>
            <span>{{ Math.round(p.percentPieces) }}%</span>
            <span>↓ {{ formatSpeed(p.downloadBps) }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, computed } from "vue";
import type { TorrentPeerStatus, TorrentStatus } from "../../../shared/types";
import { formatBytes, formatSpeed } from "../utils/format";

const torrents = ref<TorrentStatus[]>([]);
const magnetInput = ref("");
const pickedFilePath = ref("");
const torRequiredInput = ref(false);
const adding = ref(false);
const addError = ref("");
const expandedPeers = ref<Set<string>>(new Set());
const expandedFiles = ref<Set<string>>(new Set());
const peersByTorrent = ref<Record<string, TorrentPeerStatus[]>>({});

const canAdd = computed(
  () => magnetInput.value.trim().length > 0 || pickedFilePath.value.length > 0,
);

function statusLabel(t: TorrentStatus): string {
  switch (t.status) {
    case "fetching_metadata":
      return "Buscando metadados...";
    case "downloading":
      return "Baixando";
    case "seeding":
      return "Concluído (semeando)";
    case "paused":
      return "Pausado";
    case "checking":
      return "Reverificando dados...";
    case "error":
      return "Erro";
    default:
      return t.status;
  }
}

async function pickTorrentFile(): Promise<void> {
  const path = await window.api.torrents.pickTorrentFile().catch(() => null);
  if (path) {
    pickedFilePath.value = path;
    magnetInput.value = "";
  }
}

async function addTorrent(): Promise<void> {
  const source = pickedFilePath.value || magnetInput.value.trim();
  if (!source) return;
  adding.value = true;
  addError.value = "";
  try {
    const settings = await window.api.settings.load().catch(() => null);
    const destDir = settings?.outputDir || "~/Downloads";
    await window.api.torrents.add(source, destDir, torRequiredInput.value);
    magnetInput.value = "";
    pickedFilePath.value = "";
    torRequiredInput.value = false;
    await refresh();
  } catch (err) {
    addError.value =
      err instanceof Error ? err.message : "Erro ao adicionar torrent";
  } finally {
    adding.value = false;
  }
}

async function pause(t: TorrentStatus): Promise<void> {
  await window.api.torrents.pause(t.id);
  await refresh();
}
async function resume(t: TorrentStatus): Promise<void> {
  await window.api.torrents.resume(t.id);
  await refresh();
}
async function recheck(t: TorrentStatus): Promise<void> {
  await window.api.torrents.recheck(t.id);
  await refresh();
}
async function remove(t: TorrentStatus, deleteFiles: boolean): Promise<void> {
  if (!confirm(`Remover "${t.name || t.source}" da lista de torrents?`)) return;
  await window.api.torrents.remove(t.id, deleteFiles);
  await refresh();
}

function toggleFiles(t: TorrentStatus): void {
  const next = new Set(expandedFiles.value);
  if (next.has(t.id)) next.delete(t.id);
  else next.add(t.id);
  expandedFiles.value = next;
}

async function togglePeers(t: TorrentStatus): Promise<void> {
  const next = new Set(expandedPeers.value);
  if (next.has(t.id)) {
    next.delete(t.id);
  } else {
    next.add(t.id);
    peersByTorrent.value = {
      ...peersByTorrent.value,
      [t.id]: await window.api.torrents.peers(t.id),
    };
  }
  expandedPeers.value = next;
}

async function onToggleFile(
  t: TorrentStatus,
  f: { index: number },
  event: Event,
): Promise<void> {
  const checked = (event.target as HTMLInputElement).checked;
  const files = t.files ?? [];
  const indices = files
    .filter((file) => (file.index === f.index ? checked : file.selected))
    .map((file) => file.index);
  await window.api.torrents.selectFiles(t.id, indices);
  await refresh();
}

async function refresh(): Promise<void> {
  torrents.value = await window.api.torrents.list();
  for (const id of expandedPeers.value) {
    peersByTorrent.value = {
      ...peersByTorrent.value,
      [id]: await window.api.torrents.peers(id),
    };
  }
}

let pollTimer: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  void refresh();
  pollTimer = setInterval(() => void refresh(), 2000);
});
onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
});
</script>

<style scoped>
.torrent-panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px;
  height: 100%;
  overflow-y: auto;
}

.torrent-add-box {
  display: flex;
  gap: 8px;
  align-items: center;
}

.torrent-magnet-input {
  flex: 1;
  height: 34px;
  padding: 0 12px;
  border-radius: 9px;
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s ease;
}

.torrent-magnet-input:focus {
  border-color: color-mix(in srgb, var(--accent-color) 55%, var(--border-color));
}

/* Botões pill no mesmo padrão do topbar (.quick-toggle-btn / .tor-main-btn). */
.btn-secondary {
  height: 34px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 0 12px;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-card);
  color: var(--text-primary);
  cursor: pointer;
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
  transition: border-color 0.15s ease;
}

.btn-secondary:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--accent-color) 48%, var(--border-color));
}

.btn-primary {
  height: 34px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0 16px;
  border: 1px solid transparent;
  border-radius: 9px;
  background: var(--accent-color);
  color: #fff;
  cursor: pointer;
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
  transition: opacity 0.15s ease;
}

.btn-primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn-primary:disabled {
  opacity: 0.45;
  cursor: default;
}

.tor-required-check {
  height: 34px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  border-radius: 9px;
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
  transition: border-color 0.15s ease, color 0.15s ease, background 0.15s ease;
}

.tor-required-check:has(input:checked) {
  border-color: color-mix(in srgb, var(--accent-color) 48%, var(--border-color));
  background: color-mix(in srgb, var(--accent-color) 12%, var(--bg-card));
  color: var(--accent-color);
}

.tor-required-check input {
  accent-color: var(--accent-color);
}

.torrent-picked-file {
  font-size: 12px;
  color: var(--text-secondary);
  display: flex;
  gap: 6px;
  align-items: center;
}

.torrent-add-error {
  color: #ef4444;
  font-size: 12px;
}

.torrent-empty {
  color: var(--text-muted);
  padding: 32px;
  text-align: center;
  font-size: 13px;
}

.torrent-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.torrent-row {
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 12px 14px;
  background: var(--bg-card);
}

.torrent-row-main {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
}

.torrent-row-info {
  display: flex;
  flex-direction: column;
  min-width: 0;
  gap: 2px;
}

.torrent-name {
  font-weight: 700;
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 480px;
}

.torrent-meta {
  font-size: 11px;
  color: var(--text-muted);
  display: flex;
  gap: 6px;
  align-items: center;
}

.torrent-row-actions {
  display: flex;
  gap: 4px;
}

/* Mesmo padrão dos botões de ação por linha da lista de downloads (.action-btn). */
.icon-btn {
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 11px;
  transition: all 0.15s ease;
}

.icon-btn:hover {
  background: color-mix(in srgb, var(--accent-color) 15%, transparent);
  color: var(--accent-color);
  border-color: color-mix(in srgb, var(--accent-color) 40%, var(--border-color));
}

.icon-btn.danger:hover {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.12);
  border-color: rgba(239, 68, 68, 0.4);
}

.torrent-progress-bar {
  height: 6px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--border-color) 60%, transparent);
  margin-top: 10px;
  overflow: hidden;
}

.torrent-progress-fill {
  height: 100%;
  background: var(--accent-color);
  transition: width 0.4s ease;
}

.torrent-stats {
  display: flex;
  gap: 14px;
  font-size: 11px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
  margin-top: 7px;
  flex-wrap: wrap;
}

.torrent-error {
  color: #ef4444;
  font-size: 11px;
  margin-top: 6px;
}

.torrent-files,
.torrent-peers {
  margin-top: 10px;
  border-top: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
  padding-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 220px;
  overflow-y: auto;
}

.torrent-file-row {
  display: flex;
  gap: 8px;
  align-items: center;
  font-size: 11.5px;
  color: var(--text-secondary);
}

.torrent-file-row input {
  accent-color: var(--accent-color);
}

.torrent-file-path {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.torrent-file-size {
  font-variant-numeric: tabular-nums;
  color: var(--text-muted);
}

.torrent-peer-row {
  display: grid;
  grid-template-columns: 1fr 1fr 60px 100px;
  gap: 8px;
  font-size: 11px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}

.torrent-peers-empty {
  font-size: 11.5px;
  color: var(--text-muted);
}

.torrent-tor-badge {
  color: var(--accent-color);
}

.link-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  text-decoration: underline;
  cursor: pointer;
  font-size: 11.5px;
}

.link-btn:hover {
  color: var(--text-primary);
}
</style>
