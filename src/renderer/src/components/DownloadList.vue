<template>
  <div class="download-list" :class="[`density-${uiDensity}`, { 'queue-panel-collapsed': queuePanelCollapsed }]">
    <!-- Empty state -->
    <div v-if="items.length === 0 && skeletonCount === 0" class="empty-state">
      <div class="empty-icon">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none" width="48" height="48">
          <circle cx="24" cy="24" r="22" stroke="currentColor" stroke-width="1.5" opacity="0.3"/>
          <path d="M24 14 L24 30 M18 25 L24 32 L30 25" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
          <path d="M16 36 H32" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="empty-title">{{ (skeletonCount ?? 0) > 0 ? 'Preparando download' : 'Nenhum download ativo' }}</p>
      <p class="empty-sub">
        {{ (skeletonCount ?? 0) > 0 ? 'Lendo metadados e adicionando à fila...' : 'Cole um link, arraste um arquivo ou importe um container para começar' }}
      </p>
      <div class="empty-actions">
        <button class="empty-primary" @click="$emit('open-grabber')">
          <i class="pi pi-link"></i>
          Capturar links
        </button>
        <button class="empty-secondary" @click="chooseOutputDir">
          <i class="pi pi-folder"></i>
          Pasta destino
        </button>
        <button class="empty-secondary" @click="hydrate">
          <i class="pi pi-refresh"></i>
          Atualizar
        </button>
      </div>
    </div>

    <aside v-if="items.length > 0" class="package-sidebar" aria-label="Tags">
      <button
        class="package-node"
        :class="{ active: selectedPackageId === 'all' }"
        @click="selectedPackageId = 'all'"
      >
        <span class="package-status-dot ok"></span>
        <span class="package-node-label">Todos</span>
        <span class="package-node-count">{{ items.length }}</span>
      </button>
      <button
        v-for="tag in typeTags"
        :key="tag.id"
        class="package-node"
        :class="{ active: selectedPackageId === tag.id }"
        @click="selectedPackageId = tag.id"
      >
        <span class="package-color-dot" :style="{ backgroundColor: tag.color }"></span>
        <span class="package-node-label">{{ tag.label }}</span>
        <span class="package-node-count">{{ tag.count }}</span>
      </button>
    </aside>

    <!-- Download items -->
      <div
        v-if="items.length > 0 || skeletonCount > 0"
        ref="itemsContainerRef"
        class="items-container"
        @scroll="onListScroll"
      >
      <div v-if="items.length > 0 || skeletonCount > 0" class="list-toolbar">
        <span class="list-count">
          {{ orderedItems.length }} item(ns)
          <template v-if="(skeletonCount ?? 0) > 0"> · adicionando {{ skeletonCount }}</template>
        </span>
        <div class="toolbar-actions">
          <label class="toolbar-sort">
            <span>Status</span>
            <select v-model="filterStatuses" class="toolbar-select" multiple size="1" @change="persistFilters">
              <option v-for="status in statusFilterOptions" :key="status" :value="status">{{ status }}</option>
            </select>
          </label>
          <label class="toolbar-sort">
            <span>Host</span>
            <select v-model="filterHosts" class="toolbar-select" multiple size="1" @change="persistFilters">
              <option v-for="host in hostFilterOptions" :key="host" :value="host">{{ host }}</option>
            </select>
          </label>
          <label class="toolbar-sort">
            <span>Pacote</span>
            <select v-model="filterPackages" class="toolbar-select" multiple size="1" @change="persistFilters">
              <option value="">Sem pacote</option>
              <option v-for="pkg in packages" :key="pkg.id" :value="pkg.id">{{ pkg.name }}</option>
            </select>
          </label>
          <button
            v-if="hasActiveListFilters"
            class="toolbar-btn"
            title="Limpar filtros"
            @click="clearListFilters"
          >
            Limpar filtros
          </button>
          <button
            class="toolbar-btn sort-cycle-btn"
            :title="`Ordenar por ${currentSortLabel}`"
            @click="cycleSortMode"
          >
            <i class="pi pi-sort-alt"></i>
            {{ currentSortLabel }}
          </button>
          <button
            class="toolbar-btn"
            :disabled="finishedCount === 0"
            title="Remover downloads encerrados da lista"
            @click="clearFinished"
          >
            Limpar concluídos
          </button>
          <div class="display-menu-wrap">
            <button
              class="toolbar-btn"
              :class="{ active: displayMenuOpen }"
              title="Configurar exibição da lista"
              @click.stop="toggleDisplayMenu"
            >
              <i class="pi pi-sliders-h"></i>
              Exibição
            </button>
            <div v-if="displayMenuOpen" class="display-menu" @click.stop>
              <div class="display-menu-section">
                <span class="display-menu-label">Densidade</span>
                <div class="density-toggle">
                  <button
                    v-for="opt in densityOptions"
                    :key="opt.value"
                    class="density-btn"
                    :class="{ active: uiDensity === opt.value }"
                    :title="opt.label"
                    @click="setDensity(opt.value)"
                  >
                    <i :class="opt.icon"></i>
                  </button>
                </div>
              </div>
              <div class="display-menu-section">
                <span class="display-menu-label">Campos visíveis</span>
                <div class="display-fields">
                  <label
                    v-for="col in columnOptions"
                    :key="col.id"
                    class="display-field"
                  >
                    <input
                      type="checkbox"
                      :checked="hasColumn(col.id)"
                      @change="toggleColumn(col.id)"
                    />
                    <span>{{ col.label }}</span>
                  </label>
                </div>
              </div>
              <div class="display-menu-section">
                <label class="display-field display-toggle-row">
                  <input
                    type="checkbox"
                    :checked="reorderAnimations"
                    @change="toggleReorderAnimations"
                  />
                  <span>Animação suave ao reordenar</span>
                </label>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div class="items-stack">
        <div
          v-for="n in skeletonCount"
          :key="`skeleton-${n}`"
          class="skeleton-card"
        >
          <div class="skeleton-icon"></div>
          <div class="skeleton-body">
            <div class="skeleton-line skeleton-title"></div>
            <div class="skeleton-line skeleton-progress"></div>
            <div class="skeleton-line skeleton-meta"></div>
          </div>
        </div>
        <div v-if="virtualizationEnabled && topSpacerHeight > 0" :style="{ height: `${topSpacerHeight}px` }"></div>
        <TransitionGroup
          tag="div"
          name="reorder"
          class="items-stack-rows"
          :class="{ 'reorder-animate': reorderAnimationEnabled }"
        >
        <div
          v-for="item in visibleItems"
          :key="item.id"
          class="download-card"
          :class="[`status-bg-${item.status}`, { 'status-flash': flashingIds.has(item.id), 'card-pinned': item.pinned, selected: selectedDownloadIds.has(item.id) }]"
          @contextmenu.prevent="openContextMenu(item, $event)"
          @click="toggleDetailsFromCard(item, $event)"
        >
          <!-- Left: provider icon -->
          <div
            v-if="hasColumn('host') && item.moduleId === 'youtube' && item.thumbnailUrl"
            class="provider-icon provider-icon-thumb"
            :title="moduleLabel(item.moduleId)"
          >
            <img :src="item.thumbnailUrl" class="item-thumb-img" />
          </div>
          <div
            v-else-if="hasColumn('host')"
            class="provider-icon"
            v-html="getIcon(item.moduleId).svg"
            :style="providerIconStyle(item.moduleId)"
            :title="moduleLabel(item.moduleId)"
          ></div>

          <!-- Center: info -->
          <div class="item-body">
            <!-- Row 1: filename + status + actions -->
            <div class="item-header">
              <div class="item-title-wrap">
                <template v-if="hasColumn('name')">
                <span
                  class="type-icon"
                  :class="getFileIcon(item.title || item.url, undefined, item.isFolder).className"
                  :aria-label="getFileIcon(item.title || item.url, undefined, item.isFolder).alt"
                  role="img"
                ></span>
                <span class="item-title" :title="item.title">{{ item.title || item.url }}</span>
                </template>
              </div>
              <div class="item-actions">
                <span v-if="hasColumn('status')" class="status-badge" :class="`badge-${item.status}`">
                  <span class="badge-dot" :class="`dot-${item.status}`"></span>
                  {{ statusTextValue(item) }}
                </span>
                <span v-if="selectedDownloadIds.has(item.id)" class="selection-badge">
                  {{ selectedDownloadIds.size }}
                </span>
                <button
                  class="action-btn"
                  title="Mais ações"
                  @click.stop="openContextMenu(item, $event)"
                >
                  <i class="pi pi-ellipsis-v"></i>
                </button>
              </div>
            </div>

            <!-- Channel line (YouTube only) -->
            <div v-if="item.channelName" class="item-channel">
              <img v-if="item.channelThumbnailUrl" :src="item.channelThumbnailUrl" class="item-channel-avatar" />
              <span>{{ item.channelName }}</span>
            </div>

            <div v-if="showYouTubeStages(item)" class="youtube-stage-strip" aria-label="Etapas do YouTube">
              <span
                v-for="stage in youtubeStages(item)"
                :key="`${item.id}:${stage.label}`"
                class="youtube-stage"
                :class="stage.state"
              >
                <i :class="stage.icon"></i>
                {{ stage.label }}
              </span>
            </div>

            <!-- Row 2: progress bar -->
            <div v-if="hasColumn('progress')" class="progress-track">
              <div
                class="progress-fill"
                :class="{ 'progress-shimmer': item.status === 'downloading' || item.status === 'verifying' }"
                :style="{
                  width: item.percent + '%',
                  background: getProgressColor(item)
                }"
              ></div>
            </div>

            <div class="item-meta">
              <canvas
                v-if="hasSpeedSparkline(item)"
                :ref="setSparklineCanvas(item.id)"
                class="row-sparkline"
                width="90"
                height="22"
                aria-hidden="true"
              ></canvas>
              <span class="meta-percent">{{ item.percent }}%</span>

              <template v-if="item.status === 'downloading' && hasColumn('speed')">
                <span class="meta-sep">·</span>
                <span class="meta-speed">{{ formatSpeed(effectiveSpeedValue(item)) }}</span>
              </template>
              <template v-if="item.status === 'downloading' && hasColumn('eta')">
                <span class="meta-sep">·</span>
                <span class="meta-eta">{{ formatEta(effectiveEtaValue(item)) }} restante</span>
              </template>

              <template v-else-if="item.status === 'verifying'">
                <span class="meta-sep">·</span>
                <span class="meta-verifying">
                  <i class="pi pi-shield"></i>
                  Verificando {{ item.expectedHash?.algorithm?.toUpperCase() ?? 'hash' }}
                </span>
              </template>

              <template v-else-if="item.status === 'rate_limited'">
                <span class="meta-sep">·</span>
                <span class="meta-wait">
                  <i class="pi pi-clock"></i>
                  {{ item.retryAt && item.retryAt > nowTick ? formatEta(Math.ceil((item.retryAt - nowTick) / 1000)) + ' para desbloquear' : 'Bloqueado por limite de taxa' }}
                </span>
              </template>

              <template v-else-if="item.status === 'waiting_captcha'">
                <span class="meta-sep">·</span>
                <span class="meta-captcha-wait">
                  <i class="pi pi-shield"></i>
                  Aguardando resolução do captcha
                </span>
              </template>

              <template v-else-if="isWaitingRetryNow(item)">
                <span class="meta-sep">·</span>
                <span class="meta-wait">
                  <i class="pi pi-clock"></i>
                  {{ formatEta(retryCountdownNow(item)) }} para tentar novamente
                </span>
              </template>

              <template v-if="displayTotal(item) > 0 && hasColumn('size')">
                <span class="meta-sep">·</span>
                <span class="meta-size">
                  {{ formatBytes(Math.floor((item.percent / 100) * displayTotal(item))) }}
                  / {{ formatBytes(displayTotal(item)) }}
                </span>
              </template>

              <template v-if="item.isFolder && (item.children?.length ?? 0) > 0">
                <span class="meta-sep">·</span>
                <span class="meta-size">{{ item.children?.length }} item(ns)</span>
              </template>

              <template v-if="isWaitingRetryNow(item) && item.error">
                <span class="meta-sep">·</span>
                <span class="meta-wait-reason" :title="item.error">{{ item.error }}</span>
              </template>

              <template v-else-if="item.status === 'disk_full'">
                <span class="meta-sep">·</span>
                <span class="meta-disk-full">
                  <i class="pi pi-database"></i>
                  Espaço em disco insuficiente
                </span>
              </template>

              <template v-else-if="(item.status === 'error' || item.status === 'corrupted') && item.error">
                <span class="meta-sep">·</span>
                <span class="meta-error" :title="item.error">{{ item.error }}</span>
              </template>

              <template v-if="(item.maxRetries ?? 0) > 0">
                <span class="meta-sep">·</span>
                <span class="meta-retries">
                  tentativa {{ (item.retryCount ?? 0) + 1 }}/{{ (item.maxRetries ?? 0) + 1 }}
                </span>
              </template>

              <template v-if="packages.length > 0 && hasColumn('package')">
                <span class="meta-sep">·</span>
                <label class="package-picker">
                  <span
                    class="package-dot"
                    :style="{ backgroundColor: packageColor(item.packageId) }"
                  ></span>
                  <select
                    :value="item.packageId ?? ''"
                    title="Mover para pacote"
                    @change="assignPackage(item, ($event.target as HTMLSelectElement).value)"
                  >
                    <option value="">Sem pacote</option>
                    <option
                      v-for="pkg in packages"
                      :key="pkg.id"
                      :value="pkg.id"
                    >
                      {{ pkg.name }}
                    </option>
                  </select>
                </label>
              </template>
              <template v-if="hasColumn('added')">
                <span class="meta-sep">·</span>
                <span class="meta-size">Adicionado {{ formatDateTime(item.addedAt) }}</span>
              </template>
              <template v-if="item.completedAt && hasColumn('completed')">
                <span class="meta-sep">·</span>
                <span class="meta-size">Concluído {{ formatDateTime(item.completedAt) }}</span>
              </template>
              <template v-if="item.expectedHash && hasColumn('hash')">
                <span class="meta-sep">·</span>
                <span class="meta-size">{{ item.expectedHash.algorithm.toUpperCase() }}</span>
              </template>
              <span
                v-for="badge in rowBadges(item)"
                :key="`${item.id}:${badge.label}`"
                class="row-rich-badge"
                :class="badge.kind"
                :title="badge.title"
              >
                <span v-if="badge.kind === 'tor'" class="row-tor-icon" v-html="torIconSvg"></span>
                {{ badge.label }}
              </span>
            </div>

            <!-- ErrorState for specific error types -->
            <ErrorState
              v-if="item.status === 'disk_full'"
              title="Disco cheio"
              :description="item.error || 'Não há espaço suficiente para salvar o arquivo.'"
              icon="pi pi-database"
              :actions="[
                { label: 'Trocar pasta', icon: 'pi pi-folder', variant: 'primary', handler: () => chooseOutputDir() },
                { label: 'Tentar novamente', icon: 'pi pi-refresh', variant: 'secondary', handler: () => retry(item.id) },
              ]"
            />
            <ErrorState
              v-else-if="item.status === 'error' && item.error"
              :title="item.error"
              icon="pi pi-exclamation-circle"
              :actions="[
                { label: 'Tentar novamente', icon: 'pi pi-refresh', variant: 'secondary', handler: () => retry(item.id) },
              ]"
            />

            <!-- Tor ao atingir limite: chip discreto (engajamento automático) -->
            <div v-if="item.autoTorOnLimit && showTorLimitHint(item)" class="tor-limit-chip">
              <span class="row-tor-icon" v-html="torIconSvg"></span>
              <span class="tor-chip-text">Contornando limite via Tor<template v-if="item.networkRoute?.circuitChanges"> · circuito #{{ item.networkRoute.circuitChanges }}</template></span>
              <button class="tor-chip-off" title="Desativar Tor para este download" @click.stop="disableAutoTor(item)">Desativar</button>
            </div>

            <div
              v-show="item.isFolder && isExpanded(item.id) && (item.children?.length ?? 0) > 0"
              class="folder-children"
            >
              <div class="folder-tree-header">
                <span>Árvore de arquivos</span>
                <span>{{ item.children?.length ?? 0 }} item(ns)</span>
              </div>
              <VirtualRows
                class="download-child-virtual-list"
                :items="childNodes(item.children)"
                key-field="key"
                :item-height="58"
                :overscan="8"
                max-height="360px"
              >
                <template #default="{ item: node }">
                  <div
                    class="child-row"
                    :class="{ 'is-folder-row': node.isFolder }"
                  >
                    <div class="child-main">
                      <div class="child-name" :style="{ paddingInlineStart: `${node.depth * 18}px` }">
                        <span
                          class="child-icon"
                          :class="getFileIcon(node.name, node.mimeType, node.isFolder).className"
                          :aria-label="getFileIcon(node.name, node.mimeType, node.isFolder).alt"
                          role="img"
                        ></span>
                        <span class="child-name-label">{{ node.name }}</span>
                        <span v-if="node.isFolder" class="child-folder-badge">{{ node.fileCount }} item(ns)</span>
                      </div>
                      <div class="child-meta">
                        <span class="child-status">{{ childStatusText(node.status) }}</span>
                        <span class="meta-sep">·</span>
                        <span>{{ childPercent(node) }}%</span>
                        <template v-if="(node.speedBps ?? 0) > 0">
                          <span class="meta-sep">·</span>
                          <span class="meta-speed">{{ formatSpeed(node.speedBps ?? 0) }}</span>
                        </template>
                        <template v-if="(node.etaSec ?? 0) > 0">
                          <span class="meta-sep">·</span>
                          <span>{{ formatEta(node.etaSec ?? 0) }}</span>
                        </template>
                        <template v-if="node.isFolder">
                          <span class="meta-sep">·</span>
                          <span>{{ node.fileCount }} arquivo(s)</span>
                        </template>
                      </div>
                      <div class="child-track">
                        <div
                          class="child-fill"
                          :style="{ width: `${childPercent(node)}%` }"
                        ></div>
                      </div>
                    </div>
                    <span class="child-size">{{ formatBytes(node.size) }}</span>
                  </div>
                </template>
              </VirtualRows>
            </div>

            <div v-if="isDetailExpanded(item.id)" class="download-detail-panel" @click.stop>
              <div class="detail-tabs">
                <button
                  v-for="tab in detailTabOptions(item)"
                  :key="tab.id"
                  class="detail-tab"
                  :class="{ active: activeDetailTab(item.id) === tab.id }"
                  @click="setDetailTab(item, tab.id)"
                >
                  {{ tab.label }}
                </button>
              </div>

              <div v-if="activeDetailTab(item.id) === 'files'" class="detail-files-pane">
                <div class="folder-tree-header">
                  <span>Arquivos</span>
                  <span>{{ item.children?.length ?? 0 }} item(ns)</span>
                </div>
                <div v-if="!(item.children?.length)" class="queue-empty">Nenhum arquivo interno foi informado pelo provedor.</div>
                <VirtualRows
                  v-if="item.children?.length"
                  class="download-child-virtual-list"
                  :items="childNodes(item.children)"
                  key-field="key"
                  :item-height="58"
                  :overscan="10"
                  max-height="520px"
                >
                  <template #default="{ item: node }">
                    <div
                      class="child-row"
                      :class="{ 'is-folder-row': node.isFolder }"
                    >
                      <div class="child-main">
                        <div class="child-name" :style="{ paddingInlineStart: `${node.depth * 18}px` }">
                          <span
                            class="child-icon"
                            :class="getFileIcon(node.name, node.mimeType, node.isFolder).className"
                            :aria-label="getFileIcon(node.name, node.mimeType, node.isFolder).alt"
                            role="img"
                          ></span>
                          <span class="child-name-label">{{ node.name }}</span>
                          <span v-if="node.isFolder" class="child-folder-badge">{{ node.fileCount }} item(ns)</span>
                        </div>
                        <div class="child-meta">
                          <span class="child-status">{{ childStatusText(node.status) }}</span>
                          <span class="meta-sep">·</span>
                          <span>{{ childPercent(node) }}%</span>
                          <template v-if="(node.speedBps ?? 0) > 0">
                            <span class="meta-sep">·</span>
                            <span class="meta-speed">{{ formatSpeed(node.speedBps ?? 0) }}</span>
                          </template>
                          <template v-if="(node.etaSec ?? 0) > 0">
                            <span class="meta-sep">·</span>
                            <span>{{ formatEta(node.etaSec ?? 0) }}</span>
                          </template>
                        </div>
                        <div class="child-track">
                          <div
                            class="child-fill"
                            :style="{ width: `${childPercent(node)}%` }"
                          ></div>
                        </div>
                      </div>
                      <span class="child-size">{{ formatBytes(node.size) }}</span>
                    </div>
                  </template>
                </VirtualRows>
              </div>

              <div v-else-if="activeDetailTab(item.id) === 'general'" class="detail-grid">
                <div><span>ID</span><strong>{{ item.id }}</strong></div>
                <div><span>Host</span><strong>{{ moduleLabel(item.moduleId) }}</strong></div>
                <div>
                  <span>Status</span>
                  <strong class="status-detail-value" :style="{ color: statusColor(item.status) }">
                    {{ statusTextValue(item) }} · {{ statusColor(item.status) }}
                  </strong>
                </div>
                <div><span>Tamanho</span><strong>{{ formatBytes(item.size) }}</strong></div>
                <div><span>Speed</span><strong>{{ formatSpeed(effectiveSpeedValue(item)) }}</strong></div>
                <div><span>ETA</span><strong>{{ effectiveEtaValue(item) > 0 ? formatEta(effectiveEtaValue(item)) : '-' }}</strong></div>
                <div><span>Rede</span><strong>{{ networkRouteLabel(item) }}</strong></div>
                <div v-if="item.networkRoute?.isolated"><span>Circuito</span><strong>{{ item.networkRoute.proxyUsername ?? 'Isolado' }} · {{ item.networkRoute.circuitChanges ?? 0 }} troca(s)</strong></div>
                <div><span>Adicionado</span><strong>{{ formatDateTime(item.addedAt) }}</strong></div>
                <div><span>Destino</span><strong>{{ item.outputPath || '-' }}</strong></div>
                <div v-if="itemMayNeedArchivePassword(item)" class="detail-wide archive-password-editor">
                  <span>Senha do arquivo</span>
                  <div>
                    <input
                      :value="archivePasswordDrafts[item.id] ?? ''"
                      type="text"
                      placeholder="Digite a senha deste arquivo"
                      @input="archivePasswordDrafts[item.id] = ($event.target as HTMLInputElement).value"
                    />
                    <button class="toolbar-btn" @click="saveArchivePasswordFor(item.id)">Salvar senha</button>
                  </div>
                  <em>{{ archivePasswordFeedback[item.id] ?? 'Usada na extração automática de arquivos compactados.' }}</em>
                </div>
                <div class="detail-wide"><span>URL</span><strong>{{ item.url }}</strong></div>
              </div>

              <div v-else-if="activeDetailTab(item.id) === 'logs'" class="detail-log-list">
                <p v-if="!(detailLogs[item.id]?.length)">Nenhum log recente encontrado para este download.</p>
                <code v-for="line in detailLogs[item.id]" :key="line">{{ line }}</code>
              </div>

              <div v-else-if="activeDetailTab(item.id) === 'mirrors'" class="detail-action-pane">
                <span>Buscar espelhos usando o nome atual do arquivo.</span>
                <button class="toolbar-btn" @click="searchMirrorsFor(item)">Buscar mirrors</button>
              </div>

              <div v-else-if="activeDetailTab(item.id) === 'peers'" class="detail-action-pane">
                <span v-if="item.moduleId === 'torrent'">Peers serão listados aqui quando o provider torrent estiver ativo.</span>
                <span v-else>Este download não é torrent.</span>
              </div>

              <div v-else class="detail-events">
                <p v-if="!(detailEvents[item.id]?.length)">Nenhum evento persistido ainda.</p>
                <div v-for="event in sortedDetailEvents(item.id)" :key="event.id" class="detail-event">
                  <span>{{ formatDateTime(event.createdAt) }}</span>
                  <strong>{{ eventKindLabel(event.kind) }}</strong>
                  <em>{{ event.message }}</em>
                </div>
              </div>
            </div>
          </div>
        </div>
        </TransitionGroup>
        <div v-if="virtualizationEnabled && bottomSpacerHeight > 0" :style="{ height: `${bottomSpacerHeight}px` }"></div>
      </div>
    </div>

    <aside v-if="items.length > 0" class="queue-preview-panel">
      <button class="queue-panel-handle" title="Recolher painel" @click="toggleQueuePanel">
        <i class="pi" :class="queuePanelCollapsed ? 'pi-chevron-left' : 'pi-chevron-right'"></i>
      </button>
      <template v-if="!queuePanelCollapsed">
        <div class="queue-panel-section">
          <div class="queue-panel-header">
            <strong>Próximos na fila</strong>
            <button class="mini-action" :disabled="nextQueueItems.length === 0" @click="forceNextQueued">
              Forçar próximo
            </button>
          </div>
          <div v-if="nextQueueItems.length === 0" class="queue-empty">Nenhum item aguardando.</div>
          <button
            v-for="item in nextQueueItems"
            :key="`queued-${item.id}`"
            class="queue-mini-row"
            draggable="true"
            @dragstart="draggedPreviewId = item.id"
            @dragover.prevent
            @drop="dropQueuedBefore(item)"
            @click="selectDownload(item)"
          >
            <span>{{ item.title || item.url }}</span>
            <em>{{ moduleLabel(item.moduleId) }}</em>
          </button>
        </div>

        <div class="queue-panel-section">
          <div class="queue-panel-header">
            <strong>Em rate-limit</strong>
          </div>
          <div v-if="rateLimitedItems.length === 0" class="queue-empty">Nenhum host bloqueado.</div>
          <button
            v-for="item in rateLimitedItems"
            :key="`limited-${item.id}`"
            class="queue-mini-row limited"
            @click="selectDownload(item)"
          >
            <span>{{ item.title || item.url }}</span>
            <em>{{ rateLimitCountdown(item) }}</em>
          </button>
        </div>
      </template>
    </aside>

    <div
      v-if="contextMenu.visible && contextMenuItem"
      class="download-context-menu"
      :style="{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px` }"
      @click.stop
    >
      <div class="context-menu-title">
        {{ contextSelection.length > 1 ? `${contextSelection.length} itens selecionados` : contextMenuItem.title || contextMenuItem.url }}
      </div>
      <button v-if="contextCan('canPause')" @click="runContextAction('pause')"><i class="pi pi-pause"></i>Pausar</button>
      <button v-if="contextCan('canResume')" @click="runContextAction('resume')"><i class="pi pi-play"></i>Retomar</button>
      <button v-if="contextCan('canRestart')" @click="runContextAction('restart')"><i class="pi pi-replay"></i>Reiniciar do zero</button>
      <button v-if="contextCan('canForce')" @click="runContextAction('force')"><i class="pi pi-bolt"></i>Forçar agora</button>
      <button v-if="contextCan('canCancel')" @click="runContextAction('cancel')"><i class="pi pi-times"></i>Cancelar</button>
      <button @click="toggleContextPin"><i class="pi pi-star"></i>{{ contextMenuItem.pinned ? 'Desafixar' : 'Fixar no topo' }}</button>
      <button @click="toggleContextAutoTor">
        <span class="ctx-tor-icon" v-html="torIconSvg"></span>
        {{ contextMenuItem.autoTorOnLimit ? 'Desativar Tor ao atingir limite' : 'Usar Tor ao atingir limite' }}
      </button>
      <button v-if="contextMenuItem.isFolder && (contextMenuItem.children?.length ?? 0) > 0" @click="toggleContextFolder">
        <i class="pi pi-sitemap"></i>{{ isExpanded(contextMenuItem.id) ? 'Ocultar itens' : 'Mostrar itens' }}
      </button>
      <button v-if="contextCan('canOpenCaptcha')" @click="openContextCaptcha"><i class="pi pi-shield"></i>Resolver captcha</button>
      <button v-if="contextMenuItem.status === 'complete' && contextMenuItem.outputPath && isExtractableArchive(contextMenuItem.outputPath)" @click="extractContextArchive">
        <i class="pi pi-folder-plus"></i>Extrair
      </button>
      <button v-if="contextMenuItem.outputPath" @click="openContextFolder"><i class="pi pi-folder-open"></i>Abrir pasta</button>
      <button v-if="contextMenuItem.outputPath" @click="openContextFile"><i class="pi pi-external-link"></i>Abrir arquivo</button>
      <button @click="showContextUrl"><i class="pi pi-link"></i>Mostrar URL</button>
      <button @click="copyContextUrls"><i class="pi pi-copy"></i>Copiar URL</button>
      <button @click="copyContextNames"><i class="pi pi-file"></i>Copiar nome</button>
      <button @click="showContextDetails"><i class="pi pi-list"></i>Mostrar detalhes</button>
      <button v-if="contextMenuItem.moduleId === 'torrent'" @click="runContextAction('retry')"><i class="pi pi-refresh"></i>Recheck</button>

      <div class="context-menu-group">
        <span>Mover pra pacote</span>
        <button @click="assignContextPackage('')">Sem pacote</button>
        <button v-for="pkg in packages" :key="pkg.id" @click="assignContextPackage(pkg.id)">
          {{ pkg.name }}
        </button>
      </div>

      <div class="context-menu-group">
        <span>Prioridade</span>
        <button @click="setContextPriority(10)">Alta</button>
        <button @click="setContextPriority(0)">Normal</button>
        <button @click="setContextPriority(-10)">Baixa</button>
      </div>

      <button @click="setContextSpeedLimit"><i class="pi pi-gauge"></i>Limite de velocidade individual</button>
      <button v-if="contextCanTerminal" class="danger" @click="runContextAction('remove')"><i class="pi pi-trash"></i>Remover</button>
      <button v-if="contextCan('canRemoveWithFiles')" class="danger" @click="runContextAction('removeWithFiles')"><i class="pi pi-trash"></i>Remover + arquivos</button>
    </div>

    <div v-if="activeCaptchaItem" class="captcha-modal-backdrop" @click.self="closeCaptchaModal">
      <div
        ref="captchaModalRef"
        class="captcha-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="captcha-dialog-title"
        tabindex="-1"
        @keydown="onCaptchaDialogKeydown"
      >
        <div class="captcha-modal-header">
          <div>
            <strong id="captcha-dialog-title">Captcha necessário</strong>
            <p>Se houver NoPecha configurado, o app tenta antes. Se não, o captcha abre na própria página do host.</p>
          </div>
          <button class="action-btn" title="Fechar" @click="closeCaptchaModal">
            <i class="pi pi-times"></i>
          </button>
        </div>

        <div class="captcha-modal-file" :title="activeCaptchaItem.title">
          {{ activeCaptchaItem.title || activeCaptchaItem.url }}
        </div>
        <div class="captcha-modal-body">
          <p>
            O app abre uma janela modal do próprio host para evitar bloqueios de domínio do captcha.
            Resolva lá e a fila continua sozinha.
          </p>
          <button
            ref="captchaPrimaryButtonRef"
            class="captcha-modal-open-btn"
            :disabled="captchaWindowBusy"
            @click="reopenCaptchaWindow"
          >
            <i class="pi" :class="captchaWindowBusy ? 'pi-spin pi-spinner' : 'pi-external-link'"></i>
            {{ captchaWindowBusy ? 'Aguardando resolução...' : 'Abrir janela do host' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { DownloadStatus as DownloadStatusEnum } from '../../../shared/constants'
import type { DownloadChild, DownloadEvent, DownloadItem, DownloadPackage } from '../../../shared/types'
import { getFileIcon } from '../assets/file-icons'
import { getProviderIcon, getProviderColor } from '../assets/provider-icons'
import torIconSvg from '../assets/tor.svg?raw'
import { buildChildTree, flattenChildTree, type DerivedChildNode } from '../utils/child-tree'
import {
  childStatusText,
  compareDownloads,
  DOWNLOAD_SORT_OPTIONS,
  effectiveEta,
  effectiveSpeed,
  getDownloadActions,
  isClearable,
  isTerminal,
  isWaitingRetry,
  retryCountdown,
  STATUS_COLORS,
  statusText,
  type DownloadSortMode,
} from '../utils/download-display'
import { formatBytes, formatEta, formatSpeed } from '../utils/format'
import { effectiveSize } from '../utils/display-size'
import { isArchiveFilename } from '../utils/archive'
import { focusFirstDialogElement, trapDialogTab } from '../utils/dialog-focus'
import ErrorState from './ErrorState.vue'
import VirtualRows from './VirtualRows.vue'

interface ModuleSummary {
  id: string
  name: string
  color: string
}

// ── Props ──────────────────────────────────────────────────
const props = withDefaults(defineProps<{ skeletonCount?: number; torActive?: boolean }>(), {
  skeletonCount: 0,
  torActive: false,
})
const skeletonCount = computed(() => props.skeletonCount)
const torActive = computed(() => props.torActive)

// ── Emits ──────────────────────────────────────────────────
const emit = defineEmits<{
  (e: 'count-change', count: number): void
  (e: 'download-complete', payload: { id: string; outputPath: string; url?: string; title?: string; sha256Hash?: string }): void
  (e: 'global-speed', bps: number): void
  (e: 'open-grabber'): void
  (e: 'tor-changed'): void
}>()

// ── State ──────────────────────────────────────────────────
const items = ref<DownloadItem[]>([])
const stageLabels = ref<Record<string, string>>({})
const packages = ref<DownloadPackage[]>([])
const selectedPackageId = ref<'all' | 'unassigned' | string>('all')
const selectedDownloadIds = ref<Set<string>>(new Set())
const lastSelectedDownloadId = ref<string | null>(null)
const contextMenu = ref<{ visible: boolean; x: number; y: number; itemId: string | null }>({
  visible: false,
  x: 0,
  y: 0,
  itemId: null,
})
const defaultColumns = ['status', 'name', 'size', 'progress', 'speed', 'eta', 'host', 'package', 'added', 'completed', 'hash']
const visibleColumns = ref<string[]>([...defaultColumns])
const filterStatuses = ref<string[]>([])
const filterHosts = ref<string[]>([])
const filterPackages = ref<string[]>([])
const uiDensity = ref<'comfortable' | 'compact' | 'dense'>('comfortable')
const reorderAnimations = ref(true)
const queuePanelCollapsed = ref(false)
const draggedPreviewId = ref<string | null>(null)
const modulesById = ref<Record<string, ModuleSummary>>({})
const expandedFolders = ref<Record<string, boolean>>({})
const expandedDetails = ref<Record<string, boolean>>({})
type DetailTabId = 'files' | 'general' | 'logs' | 'mirrors' | 'peers' | 'history'

const detailTabs = ref<Record<string, DetailTabId>>({})
const detailLogs = ref<Record<string, string[]>>({})
const detailEvents = ref<Record<string, DownloadEvent[]>>({})
const archivePasswordDrafts = ref<Record<string, string>>({})
const archivePasswordFeedback = ref<Record<string, string>>({})
const activeCaptchaId = ref<string | null>(null)
const captchaWindowBusy = ref(false)
const captchaModalRef = ref<HTMLElement | null>(null)
const captchaPrimaryButtonRef = ref<HTMLElement | null>(null)
const sortMode = ref<DownloadSortMode>('newest')
const itemsContainerRef = ref<HTMLElement | null>(null)
const listScrollTop = ref(0)
const unsubs: Array<() => void> = []
const captchaAttemptedIds = new Set<string>()
const itemIndexById = ref<Record<string, number>>({})
const speedSamples = ref<Record<string, number[]>>({})
const rawSpeeds = ref<Record<string, number>>({})
const smoothedSpeeds = ref<Record<string, number>>({})
const captchaSolvedIds = ref<Set<string>>(new Set())
const sparklineCanvases = new Map<string, HTMLCanvasElement>()
const BROWSER_SESSION_READY_TOKEN = '__gdownloader_browser_session_ready__'
// Mutex: at most one hydrate() runs at a time; hydrateQueued ensures one follow-up
// run executes after the in-flight one finishes.
let hydrateQueued = false
let hydrateInFlight = false
let isMounted = false
let lastSpeedEmit = 0
const nowTick = ref(Date.now())
let retryTimer: number | null = null
let hydrateTimer: number | null = null
let scheduledHydrateTimer: number | null = null
const torCircuitRetryIds = new Set<string>()
const sortOptions = DOWNLOAD_SORT_OPTIONS
// Set of download IDs that recently changed status (for flash animation)
const flashingIds = ref<Set<string>>(new Set())
const downloadChildNodeCache = new WeakMap<DownloadChild[], DerivedChildNode[]>()

// ── Computed ───────────────────────────────────────────────
const packageFilteredItems = computed(() => {
  let base = items.value
  if (selectedPackageId.value !== 'all') {
    base = base.filter((item) => itemTypeTag(item).id === selectedPackageId.value)
  }
  if (filterStatuses.value.length > 0) {
    base = base.filter((item) => filterStatuses.value.includes(item.status))
  }
  if (filterHosts.value.length > 0) {
    base = base.filter((item) => filterHosts.value.includes(moduleLabel(item.moduleId)))
  }
  if (filterPackages.value.length > 0) {
    base = base.filter((item) => filterPackages.value.includes(item.packageId ?? ''))
  }
  return base
})

const statusFilterOptions = ['pending', 'downloading', 'paused', 'complete', 'error', 'waiting_captcha', 'rate_limited']
const hostFilterOptions = computed(() => {
  const hosts = new Set<string>()
  for (const item of items.value) hosts.add(moduleLabel(item.moduleId))
  return [...hosts].sort((a, b) => a.localeCompare(b))
})
const hasActiveListFilters = computed(() =>
  filterStatuses.value.length > 0 || filterHosts.value.length > 0 || filterPackages.value.length > 0
)
const typeTags = computed(() => {
  const counts = new Map<string, { id: string; label: string; color: string; count: number }>()
  for (const item of items.value) {
    const tag = itemTypeTag(item)
    const current = counts.get(tag.id) ?? { ...tag, count: 0 }
    current.count += 1
    counts.set(tag.id, current)
  }
  return [...counts.values()].sort((left, right) => right.count - left.count || left.label.localeCompare(right.label, 'pt-BR'))
})
const currentSortLabel = computed(() =>
  sortOptions.find((option) => option.value === sortMode.value)?.label ?? 'Mais recentes'
)

const orderedItems = computed(() =>
  [...packageFilteredItems.value].sort((left, right) => {
    // Pinned items float to the top
    const pinnedDiff = (right.pinned ? 1 : 0) - (left.pinned ? 1 : 0)
    if (pinnedDiff !== 0) return pinnedDiff
    return compareDownloads(left, right, sortMode.value, nowTick.value)
  })
)
const finishedCount = computed(() =>
  items.value.filter((item) => isClearable(item)).length
)
const activeCaptchaItem = computed(() =>
  items.value.find((item) => item.id === activeCaptchaId.value && item.status === DownloadStatusEnum.WaitingCaptcha) ?? null
)
const contextMenuItem = computed(() =>
  items.value.find((item) => item.id === contextMenu.value.itemId) ?? null
)
const contextSelection = computed(() => {
  const selected = items.value.filter((item) => selectedDownloadIds.value.has(item.id))
  if (selected.length > 0) return selected
  return contextMenuItem.value ? [contextMenuItem.value] : []
})
const contextCanTerminal = computed(() =>
  contextSelection.value.some((item) => actionsFor(item).canRemove)
)
const nextQueueItems = computed(() =>
  orderedItems.value
    .filter((item) => item.status === DownloadStatusEnum.Pending || isWaitingRetryNow(item))
    .slice(0, 8)
)
const rateLimitedItems = computed(() =>
  items.value
    .filter((item) => item.status === DownloadStatusEnum.RateLimited)
    .sort((left, right) => (left.retryAt ?? 0) - (right.retryAt ?? 0))
    .slice(0, 8)
)

function displayTotal(item: DownloadItem): number {
  return effectiveSize(item.size, item.children, item.isFolder)
}

function itemTypeTag(item: DownloadItem): { id: string; label: string; color: string } {
  if (item.isFolder) {
    return { id: 'folders', label: 'Pastas', color: '#f59e0b' }
  }
  const name = (item.title || item.url || '').toLowerCase().split('?')[0]
  const ext = name.includes('.') ? name.split('.').pop() ?? '' : ''
  if (['mp4', 'mkv', 'avi', 'mov', 'wmv', 'webm', 'm4v', 'flv'].includes(ext)) {
    return { id: 'videos', label: 'Vídeos', color: '#ef4444' }
  }
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'heic', 'avif'].includes(ext)) {
    return { id: 'images', label: 'Imagens', color: '#06b6d4' }
  }
  if (['zip', 'rar', '7z', 'tar', 'gz', 'tgz', 'bz2', 'xz', 'zst', 'iso'].includes(ext)) {
    return { id: 'archives', label: 'Compactados', color: '#8b5cf6' }
  }
  if (['exe', 'msi', 'dmg', 'pkg', 'appimage', 'deb', 'rpm', 'apk'].includes(ext)) {
    return { id: 'executables', label: 'Executáveis', color: '#64748b' }
  }
  if (['mp3', 'flac', 'wav', 'aac', 'm4a', 'ogg', 'opus'].includes(ext)) {
    return { id: 'audio', label: 'Áudios', color: '#22c55e' }
  }
  if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'odt'].includes(ext)) {
    return { id: 'documents', label: 'Documentos', color: '#2563eb' }
  }
  return { id: 'others', label: 'Outros', color: '#94a3b8' }
}

function selectDownload(item: DownloadItem, event?: MouseEvent): void {
  const next = new Set(selectedDownloadIds.value)
  if (event?.shiftKey && lastSelectedDownloadId.value) {
    const ids = orderedItems.value.map((entry) => entry.id)
    const start = ids.indexOf(lastSelectedDownloadId.value)
    const end = ids.indexOf(item.id)
    if (start >= 0 && end >= 0) {
      const [from, to] = start <= end ? [start, end] : [end, start]
      for (const id of ids.slice(from, to + 1)) next.add(id)
    }
  } else if (event?.metaKey || event?.ctrlKey) {
    if (next.has(item.id)) next.delete(item.id)
    else next.add(item.id)
    lastSelectedDownloadId.value = item.id
  } else {
    next.clear()
    next.add(item.id)
    lastSelectedDownloadId.value = item.id
  }
  selectedDownloadIds.value = next
}

function closeContextMenu(): void {
  contextMenu.value = { visible: false, x: 0, y: 0, itemId: null }
  displayMenuOpen.value = false
}

function openContextMenu(item: DownloadItem, event: MouseEvent): void {
  if (!selectedDownloadIds.value.has(item.id)) {
    selectDownload(item)
  }
  contextMenu.value = {
    visible: true,
    x: Math.min(event.clientX, window.innerWidth - 260),
    y: Math.min(event.clientY, window.innerHeight - 420),
    itemId: item.id,
  }
}

function contextCan(action: string): boolean {
  return contextSelection.value.some((item) => actionsFor(item)[action])
}

function hasColumn(column: string): boolean {
  return visibleColumns.value.includes(column)
}

// ── Configuração de exibição do bloco de download (densidade + campos) ──
const displayMenuOpen = ref(false)
const densityOptions: Array<{
  value: 'comfortable' | 'compact' | 'dense'
  label: string
  icon: string
}> = [
  { value: 'comfortable', label: 'Confortável', icon: 'pi pi-align-justify' },
  { value: 'compact', label: 'Compacto', icon: 'pi pi-bars' },
  { value: 'dense', label: 'Denso', icon: 'pi pi-list' },
]
const columnOptions: Array<{ id: string; label: string }> = [
  { id: 'name', label: 'Nome' },
  { id: 'status', label: 'Status' },
  { id: 'size', label: 'Tamanho' },
  { id: 'progress', label: 'Progresso' },
  { id: 'speed', label: 'Velocidade' },
  { id: 'eta', label: 'ETA' },
  { id: 'host', label: 'Host / ícone' },
  { id: 'package', label: 'Pacote' },
  { id: 'added', label: 'Adicionado' },
  { id: 'completed', label: 'Concluído' },
  { id: 'hash', label: 'Hash' },
]

function toggleDisplayMenu(): void {
  displayMenuOpen.value = !displayMenuOpen.value
}

function setDensity(value: 'comfortable' | 'compact' | 'dense'): void {
  uiDensity.value = value
  void persistDisplaySettings()
}

function toggleColumn(column: string): void {
  if (visibleColumns.value.includes(column)) {
    visibleColumns.value = visibleColumns.value.filter((item) => item !== column)
  } else {
    // Mantém a ordem canônica de defaultColumns ao reativar um campo.
    visibleColumns.value = defaultColumns.filter(
      (item) => item === column || visibleColumns.value.includes(item)
    )
  }
  void persistDisplaySettings()
}

function toggleReorderAnimations(): void {
  reorderAnimations.value = !reorderAnimations.value
  void persistDisplaySettings()
}

async function persistDisplaySettings(): Promise<void> {
  const settings = await window.api.settings.load().catch(() => null)
  if (!settings) return
  await window.api.settings
    .save({
      ...settings,
      visibleColumns: [...visibleColumns.value],
      uiDensity: uiDensity.value,
      reorderAnimations: reorderAnimations.value,
    })
    .catch(() => null)
}

function formatDateTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString('pt-BR', {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function recordSpeedSample(id: string, speed: number): void {
  const samples = speedSamples.value[id] ?? []
  speedSamples.value = {
    ...speedSamples.value,
    [id]: [...samples.slice(-29), Math.max(0, speed)],
  }
  drawSparkline(id)
}

function smoothSpeedSample(id: string, rawSpeed: number): number {
  rawSpeeds.value = { ...rawSpeeds.value, [id]: Math.max(0, Number.isFinite(rawSpeed) ? rawSpeed : 0) }
  const previous = smoothedSpeeds.value[id] ?? 0
  if (!Number.isFinite(rawSpeed) || rawSpeed <= 0) {
    const decayed = previous > 1024 ? Math.round(previous * 0.82) : 0
    smoothedSpeeds.value = { ...smoothedSpeeds.value, [id]: decayed }
    return decayed
  }
  const cappedRaw = Math.min(rawSpeed, 200 * 1024 * 1024)
  const spikeLimited = previous > 0 ? Math.min(cappedRaw, previous * 3 + 512 * 1024) : cappedRaw
  const next = previous > 0
    ? Math.round(previous * 0.72 + spikeLimited * 0.28)
    : spikeLimited
  smoothedSpeeds.value = { ...smoothedSpeeds.value, [id]: next }
  return next
}

function hasSpeedSparkline(item: DownloadItem): boolean {
  return (speedSamples.value[item.id]?.length ?? 0) > 1 || effectiveSpeedValue(item) > 0
}

function setSparklineCanvas(id: string) {
  return (el: unknown) => {
    if (el instanceof HTMLCanvasElement) {
      sparklineCanvases.set(id, el)
      drawSparkline(id)
    } else {
      sparklineCanvases.delete(id)
    }
  }
}

function drawSparkline(id: string): void {
  const canvas = sparklineCanvases.get(id)
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const samples = speedSamples.value[id] ?? []
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  if (samples.length < 2) return

  const max = Math.max(...samples, 1)
  ctx.lineWidth = 1.8
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  ctx.strokeStyle = getComputedStyle(canvas).color || '#5b7cff'
  ctx.beginPath()
  samples.forEach((speed, index) => {
    const x = (index / (samples.length - 1)) * (canvas.width - 2) + 1
    const y = canvas.height - 2 - (speed / max) * (canvas.height - 5)
    if (index === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  })
  ctx.stroke()
}

function providerIconStyle(moduleId: string): Record<string, string> {
  const color = modulesById.value[moduleId]?.color ?? getProviderColor(moduleId)
  return {
    color,
    borderColor: `${color}44`,
    background: `color-mix(in srgb, ${color} 12%, var(--bg-card))`,
  }
}

function isPremiumProvider(moduleId: string): boolean {
  return ['rapidgator', 'katfile', 'terabox'].includes(moduleId)
}

function rowBadges(item: DownloadItem): Array<{ label: string; kind: string; title: string }> {
  const badges: Array<{ label: string; kind: string; title: string }> = []
  if ((item.networkRoute?.mode === 'tor' || torActive.value) && !isTerminal(item.status)) {
    badges.push({
      label: item.networkRoute?.isolated ? 'Tor isolado' : 'Tor',
      kind: 'tor',
      title: item.networkRoute?.isolated
        ? 'Este download usa credenciais SOCKS próprias para forçar circuito separado'
        : 'Este download está usando a rota Tor ativa',
    })
  }
  if (isPremiumProvider(item.moduleId)) {
    badges.push({ label: 'Premium', kind: 'premium', title: 'Host com fluxo premium ou sessão dedicada' })
  }
  if (item.status === DownloadStatusEnum.WaitingCaptcha) {
    badges.push({ label: 'Captcha', kind: 'captcha', title: 'Aguardando resolução de captcha' })
  } else if (captchaSolvedIds.value.has(item.id)) {
    badges.push({ label: 'Captcha ok', kind: 'captcha-ok', title: 'Captcha resolvido nesta sessão' })
  }
  if (item.status === DownloadStatusEnum.RateLimited) {
    badges.push({ label: 'Rate-limited', kind: 'limited', title: 'Download aguardando janela de rate-limit' })
  }
  if (item.sequential) {
    badges.push({ label: 'Sequential', kind: 'sequential', title: 'Download sequencial ativado' })
  }
  if (item.expectedHash) {
    badges.push({
      label: item.status === DownloadStatusEnum.Complete ? `Verificado ${item.expectedHash.algorithm.toUpperCase()}` : item.expectedHash.algorithm.toUpperCase(),
      kind: 'verified',
      title: `Hash ${item.expectedHash.algorithm.toUpperCase()}: ${item.expectedHash.value}`,
    })
  }
  if (item.parallelParts && item.parallelParts > 1) {
    badges.push({ label: `${item.parallelParts} partes`, kind: 'parts', title: 'Download segmentado em partes paralelas' })
  }
  return badges
}

function networkRouteLabel(item: DownloadItem): string {
  const route = item.networkRoute
  if (route?.mode === 'tor') {
    const exit = route.exitIp || route.exitCountry
    const base = route.isolated ? 'Tor isolado' : 'Tor'
    return exit ? `${base} · ${exit}` : `${base} · ${route.proxyHost ?? '127.0.0.1'}:${route.proxyPort ?? 9150}`
  }
  return torActive.value && !isTerminal(item.status) ? 'Tor global' : 'Conexão direta'
}

function sortedDetailEvents(id: string): DownloadEvent[] {
  return [...(detailEvents.value[id] ?? [])].sort((left, right) => right.createdAt - left.createdAt)
}

function eventKindLabel(kind: string): string {
  const labels: Record<string, string> = {
    created: 'Criado',
    started: 'Iniciado',
    paused: 'Pausado',
    cancelled: 'Cancelado',
    completed: 'Concluído',
    error: 'Erro',
    removed: 'Removido',
    removed_files: 'Arquivos apagados',
    retry: 'Nova tentativa',
    forced: 'Forçado',
  }
  return labels[kind] ?? kind
}

async function persistFilters(): Promise<void> {
  const settings = await window.api.settings.load().catch(() => null)
  if (!settings) return
  await window.api.settings.save({
    ...settings,
    lastFilters: {
      statuses: filterStatuses.value,
      hosts: filterHosts.value,
      packages: filterPackages.value,
    },
  }).catch(() => null)
}

function clearListFilters(): void {
  filterStatuses.value = []
  filterHosts.value = []
  filterPackages.value = []
  void persistFilters()
}

function toggleQueuePanel(): void {
  queuePanelCollapsed.value = !queuePanelCollapsed.value
}

async function forceNextQueued(): Promise<void> {
  const next = nextQueueItems.value[0]
  if (!next) return
  await force(next.id)
}

async function dropQueuedBefore(target: DownloadItem): Promise<void> {
  if (!draggedPreviewId.value || draggedPreviewId.value === target.id) return
  const dragged = items.value.find((item) => item.id === draggedPreviewId.value)
  if (!dragged) return
  await window.api.downloads.setPriority(dragged.id, (target.priority ?? 0) + 1).catch(() => null)
  dragged.priority = (target.priority ?? 0) + 1
  draggedPreviewId.value = null
  await hydrate()
}

function rateLimitCountdown(item: DownloadItem): string {
  if (!item.retryAt || item.retryAt <= nowTick.value) return 'pronto para tentar'
  return formatEta(Math.ceil((item.retryAt - nowTick.value) / 1000))
}

const hasExpandedDownloadRows = computed(() =>
  Object.values(expandedFolders.value).some(Boolean) || Object.values(expandedDetails.value).some(Boolean)
)
const virtualizationEnabled = computed(() => orderedItems.value.length > 40)
// Animação suave de reordenação: só quando a lista não está virtualizada
// (a virtualização monta/desmonta linhas no scroll, o que brigaria com o FLIP)
// e quando o usuário não desativou nas configurações.
const reorderAnimationEnabled = computed(
  () => reorderAnimations.value && !virtualizationEnabled.value
)
const estimatedRowHeight = computed(() => hasExpandedDownloadRows.value ? 260 : 148)
const overscan = 6
const visibleRange = computed(() => {
  if (!virtualizationEnabled.value) {
    return { start: 0, end: orderedItems.value.length }
  }
  const viewportHeight = itemsContainerRef.value?.clientHeight ?? 900
  const rowHeight = estimatedRowHeight.value
  const start = Math.max(0, Math.floor(listScrollTop.value / rowHeight) - overscan)
  const visibleCount = Math.ceil(viewportHeight / rowHeight) + overscan * 2
  return {
    start,
    end: Math.min(orderedItems.value.length, start + visibleCount),
  }
})
const visibleItems = computed(() =>
  virtualizationEnabled.value
    ? orderedItems.value.slice(visibleRange.value.start, visibleRange.value.end)
    : orderedItems.value
)
const topSpacerHeight = computed(() =>
  virtualizationEnabled.value ? visibleRange.value.start * estimatedRowHeight.value : 0
)
const bottomSpacerHeight = computed(() =>
  virtualizationEnabled.value
    ? Math.max(0, (orderedItems.value.length - visibleRange.value.end) * estimatedRowHeight.value)
    : 0
)

// ── Lifecycle ──────────────────────────────────────────────
onMounted(async () => {
  isMounted = true
  window.addEventListener('click', closeContextMenu)
  window.addEventListener('blur', closeContextMenu)
  window.addEventListener('keydown', onQueuePanelHotkey)
  retryTimer = window.setInterval(() => {
    nowTick.value = Date.now()
    for (const item of items.value) {
      if (item.status === DownloadStatusEnum.Downloading) {
        recordSpeedSample(item.id, effectiveSpeedValue(item))
      }
    }
    const totalSpeed = items.value
      .filter((item) => item.status === DownloadStatusEnum.Downloading)
      .reduce((sum, item) => sum + effectiveSpeedValue(item), 0)
    emit('global-speed', totalSpeed)
  }, 1000)
  hydrateTimer = window.setInterval(() => {
    if (!isMounted) return
    const hasLiveQueue = items.value.some((item) => !isTerminal(item.status))
    if (!hasLiveQueue && items.value.length > 0) return
    void hydrate()
  }, 4000)

  // Load module metadata for labels
  const modules = await window.api.modules.list().catch(() => [])
  modulesById.value = modules.reduce<Record<string, ModuleSummary>>((acc, mod) => {
    acc[mod.id] = { id: mod.id, name: mod.name, color: mod.color }
    return acc
  }, {})

  packages.value = await window.api.packages.list().catch(() => [])
  const settings = await window.api.settings.load().catch(() => null)
  if (Array.isArray(settings?.visibleColumns) && settings.visibleColumns.length > 0) {
    const known = new Set(defaultColumns)
    visibleColumns.value = settings.visibleColumns.filter((column) => known.has(column))
  }
  filterStatuses.value = settings?.lastFilters?.statuses ?? []
  filterHosts.value = settings?.lastFilters?.hosts ?? []
  filterPackages.value = settings?.lastFilters?.packages ?? []
  uiDensity.value = settings?.uiDensity ?? 'comfortable'
  reorderAnimations.value = settings?.reorderAnimations ?? true

  // Load existing downloads from backend
  await hydrate()

  // Real-time progress events
  unsubs.push(
    window.api.downloads.on('download:progress', (event: unknown) => {
      const ev = event as {
        type?: string
        id: string
        bytes?: number
        total?: number
        speed?: number
        eta?: number
        child_path?: string
        status?: string
        child_filename?: string
        child_bytes?: number
        child_total?: number
        child_speed?: number
        child_eta?: number
      }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      if (idx >= 0) {
        const rawTotal = ev.total ?? items.value[idx].size
        const isSyntheticYoutubeProgress = items.value[idx].moduleId === 'youtube' && rawTotal === 10_000
        const total = rawTotal
        const displaySize = isSyntheticYoutubeProgress ? items.value[idx].size : rawTotal
        const bytes = ev.bytes ?? 0
        let nextChildren = items.value[idx].children
        if (items.value[idx].moduleId === 'youtube' && ev.child_filename) {
          stageLabels.value = {
            ...stageLabels.value,
            [ev.id]: ev.child_filename,
          }
        }
        if (ev.child_filename && nextChildren?.length) {
          nextChildren = nextChildren.map((child) => {
            const matches = ev.child_path
              ? child.path === ev.child_path
                || child.sourceUrl === ev.child_path
                || (items.value[idx].moduleId === 'youtube'
                  && !!child.sourceUrl
                  && sameYouTubeSelection(child.sourceUrl, ev.child_path))
              : child.filename === ev.child_filename

            if (!matches) {
              return child.status === DownloadStatusEnum.Downloading
                ? { ...child, speedBps: 0, etaSec: 0 }
                : child
            }

            const childTotal = ev.child_total ?? child.size ?? 0
            const childBytes = Math.max(child.bytesDownloaded ?? 0, ev.child_bytes ?? child.bytesDownloaded ?? 0)
            const childSpeed = smoothSpeedSample(`${ev.id}:${child.path ?? child.sourceUrl ?? child.filename}`, ev.child_speed ?? child.speedBps ?? 0)
            const childStatus =
              childTotal > 0 && childBytes >= childTotal
                ? DownloadStatusEnum.Complete
                : DownloadStatusEnum.Downloading

            return {
              ...child,
              bytesDownloaded: childBytes,
              speedBps: childSpeed,
              etaSec: ev.child_eta ?? child.etaSec ?? 0,
              status: childStatus
            }
          })
        }

        const isFolder = items.value[idx].isFolder && (nextChildren?.length ?? 0) > 0
        const aggregatedChildBytes = isFolder
          ? nextChildren!.reduce((sum, child) => sum + (child.bytesDownloaded ?? 0), 0)
          : bytes
        const rawAggregatedChildSpeed = isFolder
          ? nextChildren!.reduce((sum, child) => sum + (child.speedBps ?? 0), 0)
          : (ev.speed ?? 0)
        const aggregatedChildSpeed = smoothSpeedSample(ev.id, rawAggregatedChildSpeed)
        const computedPercent = total > 0
          ? Math.min(100, Math.floor((aggregatedChildBytes / total) * 100))
          : items.value[idx].percent
        const aggregatedPercent = items.value[idx].status === DownloadStatusEnum.Downloading
          ? Math.max(items.value[idx].percent ?? 0, computedPercent)
          : computedPercent
        const aggregatedEta = aggregatedChildSpeed > 0 && total > aggregatedChildBytes
          ? Math.floor((total - aggregatedChildBytes) / aggregatedChildSpeed)
          : 0

        patchItemAt(idx, {
          percent: aggregatedPercent,
          speedBps: aggregatedChildSpeed,
          etaSec: isFolder ? aggregatedEta : (ev.eta ?? 0),
          lastProgressAt: Date.now(),
          status: (ev.status as DownloadItem['status']) ?? items.value[idx].status,
          size: displaySize > 0 ? displaySize : items.value[idx].size,
          // Keep parent bytes implicit in percent/size, but base folder progress on the sum of children.
          children: nextChildren
        })
        recordSpeedSample(ev.id, aggregatedChildSpeed)
        // Somar speed de todos os itens ativos (throttled a 200ms para não sobrecarregar)
        const now = Date.now()
        if (now - lastSpeedEmit >= 120) {
          lastSpeedEmit = now
          const totalSpeed = items.value
            .filter((i) => i.status === DownloadStatusEnum.Downloading)
            .reduce((sum, i) => sum + effectiveSpeedValue(i), 0)
          emit('global-speed', totalSpeed)
        }
      } else {
        // Unknown item — refresh list
        scheduleHydrate(100)
      }
    })
  )

  unsubs.push(
    window.api.downloads.on('download:verifying', (event: unknown) => {
      const ev = event as {
        id: string
        bytes_done?: number
        bytes_total?: number
      }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      if (idx < 0) {
        scheduleHydrate(100)
        return
      }

      const total = ev.bytes_total ?? items.value[idx].size
      const bytesDone = ev.bytes_done ?? 0
      const percent = total > 0
        ? Math.min(100, Math.floor((bytesDone / total) * 100))
        : items.value[idx].percent

      patchItemAt(idx, {
        status: DownloadStatusEnum.Verifying,
        percent,
        size: total > 0 ? total : items.value[idx].size,
        speedBps: 0,
        etaSec: 0,
        lastProgressAt: Date.now(),
      })
    })
  )

  unsubs.push(
    window.api.downloads.on('download:complete', (event: unknown) => {
      const ev = event as { id: string; path?: string; outputPath?: string }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      const outputPath = ev.path ?? ev.outputPath ?? ''
      if (idx >= 0) {
        recordSpeedSample(ev.id, 0)
        const restStages = { ...stageLabels.value }
        delete restStages[ev.id]
        stageLabels.value = restStages
        patchItemAt(idx, {
          status: DownloadStatusEnum.Complete,
          percent: 100,
          speedBps: 0,
          etaSec: 0,
          outputPath
        })
        emit('download-complete', {
          id: ev.id,
          outputPath,
          url: items.value[idx].url,
          title: items.value[idx].title,
          sha256Hash: items.value[idx].expectedHash?.algorithm === 'sha256'
            ? items.value[idx].expectedHash?.value
            : undefined,
        })

        // Auto-extract if enabled
        if (outputPath) {
          void window.api.settings.load().then((settings) => {
            if (!settings.autoExtract) return
            const passwords = (settings as { passwordList?: string[] }).passwordList ?? []
            return window.api.archive.autoExtract(outputPath, passwords).then((result) => {
              if (result.success && result.outputDir) {
                void window.api.system.notify('Extração concluída', result.outputDir).catch(() => null)
              } else if (result.error === 'WRONG_PASSWORD') {
                void window.api.system.notify('Extração falhou', 'Nenhuma senha funcionou').catch(() => null)
              }
            })
          }).catch(() => null)
        }
      }
      scheduleHydrate(900)
    })
  )

  unsubs.push(
    window.api.downloads.on('download:error', (event: unknown) => {
      const ev = event as { id: string; message?: string; error?: string }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      if (idx >= 0) {
        recordSpeedSample(ev.id, 0)
        const restStages = { ...stageLabels.value }
        delete restStages[ev.id]
        stageLabels.value = restStages
        patchItemAt(idx, {
          status: DownloadStatusEnum.Error,
          speedBps: 0,
          etaSec: 0,
          error: ev.message ?? ev.error ?? 'Erro desconhecido'
        })
      }
      scheduleHydrate(900)
    })
  )

  // Status change events (includes rate_limited + waiting_captcha metadata)
  unsubs.push(
    window.api.downloads.on('download:status', (event: unknown) => {
      const ev = event as {
        type?: string
        id: string
        status?: DownloadItem['status']
        retry_at?: number
        captcha_type?: string
        captcha_sitekey?: string
        captcha_page_url?: string
        error?: string
      }
      if (!ev?.id) return
      if (ev.type === 'status_changed') {
        upsertById(ev.id, {
          status: ev.status,
          retryAt: ev.retry_at ? ev.retry_at * 1000 : undefined,
          captchaType: ev.captcha_type ?? undefined,
          captchaSitekey: ev.captcha_sitekey ?? undefined,
          captchaPageUrl: ev.captcha_page_url ?? undefined,
          error: ev.error ?? '',
          speedBps: 0,
          etaSec: 0,
        })
        // Flash animation for status change
        flashingIds.value = new Set([...flashingIds.value, ev.id])
        setTimeout(() => {
          flashingIds.value = new Set([...flashingIds.value].filter((id) => id !== ev.id))
        }, 400)
        void maybeResolveCaptchaById(ev.id)
        if (props.torActive && ev.status === DownloadStatusEnum.RateLimited && !torCircuitRetryIds.has(ev.id)) {
          torCircuitRetryIds.add(ev.id)
          void window.api.tor.newIdentity()
            .then(() => retry(ev.id))
            .catch(() => null)
        }
        if (ev.status && ev.status !== DownloadStatusEnum.RateLimited) {
          torCircuitRetryIds.delete(ev.id)
        }
        if (ev.status === DownloadStatusEnum.RateLimited) {
          maybeAutoEngageTor()
        }
        scheduleHydrate(900)
      } else {
        scheduleHydrate(300)
      }
    })
  )

  unsubs.push(
    window.api.downloads.on('download:cancelled', (event: unknown) => {
      const ev = event as { id: string }
      if (!ev?.id) return
      upsertById(ev.id, { status: DownloadStatusEnum.Cancelled })
      scheduleHydrate(900)
    })
  )
})

onUnmounted(() => {
  isMounted = false
  if (retryTimer !== null) {
    window.clearInterval(retryTimer)
    retryTimer = null
  }
  if (hydrateTimer !== null) {
    window.clearInterval(hydrateTimer)
    hydrateTimer = null
  }
  if (scheduledHydrateTimer !== null) {
    window.clearTimeout(scheduledHydrateTimer)
    scheduledHydrateTimer = null
  }
  window.removeEventListener('click', closeContextMenu)
  window.removeEventListener('blur', closeContextMenu)
  window.removeEventListener('keydown', onQueuePanelHotkey)
  for (const unsub of unsubs) unsub()
})

function onQueuePanelHotkey(event: KeyboardEvent): void {
  const target = event.target as HTMLElement | null
  if (target?.matches('input, textarea, select, [contenteditable="true"]')) return
  if (event.key.toLowerCase() === 'q') {
    queuePanelCollapsed.value = !queuePanelCollapsed.value
  }
}

watch(activeCaptchaItem, (item) => {
  if (!item) {
    return
  }
  nextTick(() => {
    captchaPrimaryButtonRef.value?.focus()
    focusFirstDialogElement(captchaModalRef.value)
  })
})

// Dispara o Tor automático para downloads com a flag ligada que bateram no limite.
watch(
  () => items.value.map((item) => `${item.id}:${item.status}:${item.autoTorOnLimit ? 1 : 0}`).join(','),
  () => maybeAutoEngageTor(),
)

function mergeHydratedChildren(existing?: DownloadChild[], fresh?: DownloadChild[]): DownloadChild[] | undefined {
  if (!fresh) return existing
  if (!existing?.length) return fresh

  return fresh.map((child) => {
    const previous = existing.find((candidate) =>
      (!!child.path && candidate.path === child.path)
      || (!!child.sourceUrl && candidate.sourceUrl === child.sourceUrl)
      || (child.sourceUrl && candidate.sourceUrl && sameYouTubeSelection(candidate.sourceUrl, child.sourceUrl))
      || candidate.filename === child.filename
    )
    if (!previous) return child

    return {
      ...child,
      bytesDownloaded: Math.max(previous.bytesDownloaded ?? 0, child.bytesDownloaded ?? 0),
      speedBps: child.speedBps ?? previous.speedBps,
      etaSec: child.etaSec ?? previous.etaSec,
      status: child.status ?? previous.status,
    }
  })
}

function mergeHydratedDownload(existing: DownloadItem, fresh: DownloadItem): DownloadItem {
  if (existing.status !== DownloadStatusEnum.Downloading || fresh.status !== DownloadStatusEnum.Downloading) {
    return {
      ...fresh,
      children: mergeHydratedChildren(existing.children, fresh.children),
    }
  }

  return {
    ...fresh,
    percent: Math.max(existing.percent ?? 0, fresh.percent ?? 0),
    speedBps: Math.max(existing.speedBps ?? 0, fresh.speedBps ?? 0),
    etaSec: fresh.etaSec || existing.etaSec,
    children: mergeHydratedChildren(existing.children, fresh.children),
  }
}

// ── Data methods ───────────────────────────────────────────
async function hydrate(): Promise<void> {
  if (hydrateInFlight) {
    hydrateQueued = true
    return
  }
  hydrateInFlight = true
  try {
    if (!isMounted) return
    const fresh: DownloadItem[] = await window.api.downloads.list().catch(() => [])
    const freshById = new Map<string, DownloadItem>(fresh.map((item) => [item.id, item]))

    for (let i = items.value.length - 1; i >= 0; i--) {
      if (!freshById.has(items.value[i].id)) {
        items.value.splice(i, 1)
      }
    }

    for (const freshItem of fresh) {
      const idx = items.value.findIndex((i) => i.id === freshItem.id)
      if (idx >= 0) {
        Object.assign(items.value[idx], mergeHydratedDownload(items.value[idx], freshItem))
      } else {
        items.value.push(freshItem)
      }
    }

    rebuildItemIndex()

    for (const item of items.value) {
      if (item.status !== DownloadStatusEnum.WaitingCaptcha) {
        captchaAttemptedIds.delete(item.id)
        if (activeCaptchaId.value === item.id) {
          activeCaptchaId.value = null
        }
      }
    }

    for (const item of items.value) {
      if (item.status === DownloadStatusEnum.WaitingCaptcha) {
        void maybeResolveCaptcha(item)
      }
    }

    emit('count-change', items.value.length)
  } finally {
    hydrateInFlight = false
    if (hydrateQueued) {
      hydrateQueued = false
      if (isMounted) void hydrate()
    }
  }
}

function scheduleHydrate(delayMs = 700): void {
  if (!isMounted) return
  if (scheduledHydrateTimer !== null) {
    window.clearTimeout(scheduledHydrateTimer)
  }
  scheduledHydrateTimer = window.setTimeout(() => {
    scheduledHydrateTimer = null
    void hydrate()
  }, delayMs)
}

function cycleSortMode(): void {
  const index = sortOptions.findIndex((option) => option.value === sortMode.value)
  const next = sortOptions[(index + 1) % sortOptions.length]
  sortMode.value = next?.value ?? 'newest'
}

async function assignPackage(item: DownloadItem, packageId: string): Promise<void> {
  if (packageId) {
    await window.api.packages.assign(packageId, item.id).catch(() => null)
  } else {
    await window.api.packages.unassign(item.id).catch(() => null)
  }
  const idx = itemIndexById.value[item.id] ?? -1
  if (idx >= 0) {
    patchItemAt(idx, { packageId: packageId || undefined })
  }
}

function packageColor(packageId: string | undefined): string {
  if (!packageId) return '#9ca3af'
  return packages.value.find((pkg) => pkg.id === packageId)?.color ?? '#9ca3af'
}

function upsertById(id: string, patch: Partial<DownloadItem>): void {
  const idx = itemIndexById.value[id] ?? -1
  if (idx === -1) {
    void hydrate()
    return
  }
  patchItemAt(idx, patch)
}

function patchItemAt(idx: number, patch: Partial<DownloadItem>): DownloadItem | null {
  const item = items.value[idx]
  if (!item) {
    return null
  }
  Object.assign(item, patch)
  return item
}

function rebuildItemIndex(): void {
  itemIndexById.value = Object.fromEntries(items.value.map((item, index) => [item.id, index]))
}

function onListScroll(): void {
  listScrollTop.value = itemsContainerRef.value?.scrollTop ?? 0
}

// ── Actions ────────────────────────────────────────────────
async function cancel(id: string): Promise<void> {
  await window.api.downloads.cancel(id).catch(() => null)
  await hydrate()
}

async function pause(id: string): Promise<void> {
  await window.api.downloads.pause(id).catch(() => null)
  await hydrate()
}

async function resume(id: string): Promise<void> {
  await window.api.downloads.resume(id).catch(() => null)
  await hydrate()
}

async function retry(id: string): Promise<void> {
  await window.api.downloads.retry(id).catch(() => null)
  await hydrate()
}

// ── Tor ao atingir o limite (por download) ─────────────────────
const torEngaging = ref<Set<string>>(new Set())
const autoTorAttempted = new Set<string>()
const torBootstrapPercent = ref(0)
let torBootstrapTimer: ReturnType<typeof setInterval> | null = null

function startTorBootstrapPolling(): void {
  if (torBootstrapTimer) return
  torBootstrapTimer = setInterval(async () => {
    torBootstrapPercent.value = await window.api.tor.bootstrapProgress().catch(() => 0)
    if (torBootstrapPercent.value >= 100 && torBootstrapTimer) {
      clearInterval(torBootstrapTimer)
      torBootstrapTimer = null
    }
  }, 700)
}

// Mostra o aviso/CTA do Tor quando o download bateu no limite do servidor.
function showTorLimitHint(item: DownloadItem): boolean {
  return item.status === DownloadStatusEnum.RateLimited || isWaitingRetryNow(item)
}

function setItemAutoTor(id: string, enabled: boolean): void {
  const item = items.value.find((entry) => entry.id === id)
  if (item) item.autoTorOnLimit = enabled
}

// Liga o Tor para este download e segue baixando: marca a flag, conecta o Tor
// (global) e retenta. O backend então rotaciona o circuito a cada limite até
// concluir.
async function engageTorForDownload(item: DownloadItem): Promise<void> {
  if (torEngaging.value.has(item.id)) return
  torEngaging.value = new Set(torEngaging.value).add(item.id)
  startTorBootstrapPolling()
  try {
    await window.api.downloads.setAutoTor(item.id, true).catch(() => null)
    setItemAutoTor(item.id, true)
    // Inicia o daemon Tor apenas para roteamento isolado — NÃO altera o proxy global.
    await window.api.tor.ensureRunning()
    await window.api.downloads.retry(item.id).catch(() => null)
    await hydrate()
  } catch (error) {
    const item2 = items.value.find((entry) => entry.id === item.id)
    if (item2) {
      item2.error = error instanceof Error ? error.message : 'Falha ao conectar ao Tor'
    }
  } finally {
    const next = new Set(torEngaging.value)
    next.delete(item.id)
    torEngaging.value = next
  }
}

async function disableAutoTor(item: DownloadItem): Promise<void> {
  await window.api.downloads.setAutoTor(item.id, false).catch(() => null)
  setItemAutoTor(item.id, false)
}

// Auto: quando qualquer download bate no limite, dispara o Tor isolado
// automaticamente (uma vez por download, independente do proxy global).
function maybeAutoEngageTor(): void {
  for (const item of items.value) {
    if (
      showTorLimitHint(item) &&
      !item.autoTorOnLimit &&
      !torEngaging.value.has(item.id) &&
      !autoTorAttempted.has(item.id)
    ) {
      autoTorAttempted.add(item.id)
      void engageTorForDownload(item)
    }
  }
}

function itemMayNeedArchivePassword(item: DownloadItem): boolean {
  if (isArchiveFilename(item.title)) return true
  return (item.children ?? []).some((child) => isArchiveFilename(child.filename))
}

async function saveArchivePasswordFor(id: string): Promise<void> {
  const password = (archivePasswordDrafts.value[id] ?? '').trim()
  if (!password) {
    archivePasswordFeedback.value = { ...archivePasswordFeedback.value, [id]: 'Informe uma senha antes de salvar.' }
    return
  }
  await window.api.archivePasswords.import([password])
  archivePasswordFeedback.value = { ...archivePasswordFeedback.value, [id]: 'Senha salva e será testada na extração automática.' }
}

async function force(id: string): Promise<void> {
  await window.api.downloads.force(id).catch(() => null)
  await hydrate()
}

async function restart(id: string): Promise<void> {
  await window.api.downloads.restart(id).catch(() => null)
  await hydrate()
}

async function remove(id: string): Promise<void> {
  await window.api.downloads.remove(id).catch(() => null)
  await hydrate()
}

async function removeWithFiles(id: string): Promise<void> {
  if (!window.confirm('Remover da lista e apagar os arquivos físicos deste download?')) {
    return
  }
  await window.api.downloads.removeWithFiles(id).catch(() => null)
  await hydrate()
}

async function clearFinished(): Promise<void> {
  await window.api.downloads.clearFinished().catch(() => null)
  await hydrate()
}

async function togglePin(id: string): Promise<void> {
  const idx = itemIndexById.value[id] ?? -1
  if (idx >= 0) {
    // Optimistic update
    patchItemAt(idx, { pinned: !items.value[idx].pinned })
  }
  await window.api.downloads.togglePin(id).catch(() => null)
}

async function runContextAction(action: 'pause' | 'resume' | 'restart' | 'force' | 'retry' | 'cancel' | 'remove' | 'removeWithFiles'): Promise<void> {
  const targets = [...contextSelection.value]
  closeContextMenu()
  for (const item of targets) {
    if (action === 'pause' && actionsFor(item).canPause) await pause(item.id)
    if (action === 'resume' && actionsFor(item).canResume) await resume(item.id)
    if (action === 'restart' && actionsFor(item).canRestart) await restart(item.id)
    if (action === 'force' && actionsFor(item).canForce) await force(item.id)
    if (action === 'retry' && actionsFor(item).canRetry) await retry(item.id)
    if (action === 'cancel' && actionsFor(item).canCancel) await cancel(item.id)
    if (action === 'remove' && actionsFor(item).canRemove) await remove(item.id)
    if (action === 'removeWithFiles' && actionsFor(item).canRemoveWithFiles) await removeWithFiles(item.id)
  }
  await hydrate()
}

async function toggleContextPin(): Promise<void> {
  const item = contextMenuItem.value
  closeContextMenu()
  if (item) await togglePin(item.id)
}

function toggleContextFolder(): void {
  const item = contextMenuItem.value
  closeContextMenu()
  if (item) toggleFolder(item.id)
}

async function toggleContextAutoTor(): Promise<void> {
  const item = contextMenuItem.value
  closeContextMenu()
  if (!item) return
  if (item.autoTorOnLimit) {
    await disableAutoTor(item)
  } else {
    // Pré-ativa para este download. Se já estiver no limite, engata o Tor agora;
    // caso contrário, aguarda o limite acontecer (auto-watcher cuida disso).
    await window.api.downloads.setAutoTor(item.id, true).catch(() => null)
    setItemAutoTor(item.id, true)
    if (showTorLimitHint(item)) {
      void engageTorForDownload(item)
    }
  }
}

function openContextCaptcha(): void {
  const item = contextMenuItem.value
  closeContextMenu()
  if (item) openCaptcha(item.id)
}

async function extractContextArchive(): Promise<void> {
  const path = contextMenuItem.value?.outputPath
  closeContextMenu()
  if (path) await extract(path)
}

function openContextFolder(): void {
  const path = contextMenuItem.value?.outputPath
  closeContextMenu()
  if (path) openFolder(path)
}

function openContextFile(): void {
  const path = contextMenuItem.value?.outputPath
  closeContextMenu()
  if (path) void window.api.openPath(path).catch(() => null)
}

function showContextUrl(): void {
  const url = contextMenuItem.value?.url
  closeContextMenu()
  if (url) window.alert(url)
}

function showContextDetails(): void {
  const item = contextMenuItem.value
  closeContextMenu()
  if (item) toggleDetails(item)
}

async function copyContextUrls(): Promise<void> {
  const targets = [...contextSelection.value]
  closeContextMenu()
  if (targets.length === 1) {
    await copyUrl(targets[0])
    return
  }
  const payload = targets.map((item) => item.url).join('\n')
  if (payload) await window.api.clipboard.writeText(payload).catch(() => null)
}

async function copyContextNames(): Promise<void> {
  const payload = contextSelection.value.map((item) => item.title || item.url).join('\n')
  closeContextMenu()
  if (payload) await window.api.clipboard.writeText(payload).catch(() => null)
}

async function assignContextPackage(packageId: string): Promise<void> {
  const targets = [...contextSelection.value]
  closeContextMenu()
  for (const item of targets) {
    await assignPackage(item, packageId)
  }
}

async function setContextPriority(priority: number): Promise<void> {
  const targets = [...contextSelection.value]
  closeContextMenu()
  for (const item of targets) {
    await window.api.downloads.setPriority(item.id, priority).catch(() => null)
    const idx = itemIndexById.value[item.id] ?? -1
    if (idx >= 0) patchItemAt(idx, { priority })
  }
  await hydrate()
}

async function setContextSpeedLimit(): Promise<void> {
  const raw = window.prompt('Limite em KB/s para o(s) item(ns). Use 0 para sem limite.', '0')
  if (raw === null) return
  const value = Math.max(0, Math.trunc(Number(raw) || 0))
  const targets = [...contextSelection.value]
  closeContextMenu()
  for (const item of targets) {
    await window.api.downloads.setSpeedLimit(item.id, value).catch(() => null)
  }
  await hydrate()
}

function chooseOutputDir(): void {
  window.api.settings.chooseDirectory().then((dir) => {
    if (dir) {
      window.api.settings.load().then((settings) => {
        window.api.settings.save({ ...settings, outputDir: dir }).catch(() => null)
      }).catch(() => null)
    }
  }).catch(() => null)
}

async function copyUrl(item: DownloadItem): Promise<void> {
  const folderUrls = (item.children ?? [])
    .map((child: DownloadChild) => child.sourceUrl?.trim() ?? '')
    .filter((url) => url.length > 0)

  const payload = item.isFolder && folderUrls.length > 0
    ? folderUrls.join('\n')
    : item.url

  await window.api.clipboard.writeText(payload).catch(() => null)
}

async function extract(filePath: string): Promise<void> {
  try {
    const extractedPath = await window.api.archive.extract(filePath)
    await window.api.system.notify('Extração concluída', extractedPath).catch(() => null)
    await window.api.openPath(extractedPath).catch(() => null)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    await window.api.system.notify('Falha na extração', message).catch(() => null)
  }
}

function toggleFolder(id: string): void {
  expandedFolders.value = {
    ...expandedFolders.value,
    [id]: !expandedFolders.value[id]
  }
}

function toggleDetailsFromCard(item: DownloadItem, event: MouseEvent): void {
  const target = event.target as HTMLElement | null
  if (target?.closest('button,input,select,a,label,.download-detail-panel,.folder-children,.captcha-row')) {
    return
  }
  if (event.metaKey || event.ctrlKey || event.shiftKey) {
    selectDownload(item, event)
    return
  }
  if (!selectedDownloadIds.value.has(item.id) || selectedDownloadIds.value.size > 1) {
    selectDownload(item)
  }
  toggleDetails(item)
}

function toggleDetails(item: DownloadItem): void {
  expandedDetails.value = {
    ...expandedDetails.value,
    [item.id]: !expandedDetails.value[item.id],
  }
  if (expandedDetails.value[item.id]) {
    detailTabs.value = {
      ...detailTabs.value,
      [item.id]: detailTabs.value[item.id] ?? defaultDetailTab(item),
    }
    void loadDetailData(item)
  }
}

function isDetailExpanded(id: string): boolean {
  return !!expandedDetails.value[id]
}

function defaultDetailTab(item: DownloadItem): DetailTabId {
  return item.isFolder && (item.children?.length ?? 0) > 0 ? 'files' : 'general'
}

function activeDetailTab(id: string): DetailTabId {
  return detailTabs.value[id] ?? 'general'
}

function detailTabOptions(item: DownloadItem): Array<{ id: DetailTabId; label: string }> {
  const tabs: Array<{ id: DetailTabId; label: string }> = []
  if (item.isFolder && (item.children?.length ?? 0) > 0) {
    tabs.push({ id: 'files', label: 'Arquivos' })
  }
  tabs.push(
    { id: 'general', label: 'Geral' },
    { id: 'logs', label: 'Logs' },
  )
  if (item.moduleId !== 'youtube') tabs.push({ id: 'mirrors', label: 'Mirrors' })
  if (item.moduleId === 'torrent') tabs.push({ id: 'peers', label: 'Peers' })
  tabs.push({ id: 'history', label: 'Histórico' })
  return tabs
}

function setDetailTab(item: DownloadItem, tab: DetailTabId): void {
  detailTabs.value = { ...detailTabs.value, [item.id]: tab }
  void loadDetailData(item)
}

async function loadDetailData(item: DownloadItem): Promise<void> {
  const tab = activeDetailTab(item.id)
  if (tab === 'logs' && !detailLogs.value[item.id]) {
    const log = await window.api.logs.tail(500).catch(() => ({ path: '', lines: [] }))
    const needles = [item.id, item.title, item.url].filter(Boolean).map((value) => value.toLowerCase())
    detailLogs.value = {
      ...detailLogs.value,
      [item.id]: log.lines.filter((line) => needles.some((needle) => line.toLowerCase().includes(needle))).slice(-80),
    }
  }
  if (tab === 'history' && !detailEvents.value[item.id]) {
    const events = await window.api.downloads.events(item.id).catch(() => [])
    detailEvents.value = { ...detailEvents.value, [item.id]: events }
  }
}

async function searchMirrorsFor(item: DownloadItem): Promise<void> {
  const filename = item.title || item.url
  await window.api.mirrors.search(filename).catch(() => null)
  await window.api.system.notify('Busca de mirrors iniciada', filename).catch(() => null)
}

function effectiveSpeedValue(item: DownloadItem): number {
  return effectiveSpeed(item, nowTick.value)
}

function effectiveEtaValue(item: DownloadItem): number {
  return effectiveEta(item, nowTick.value)
}

type YouTubeStageState = 'done' | 'current' | 'pending'

function showYouTubeStages(item: DownloadItem): boolean {
  return item.moduleId === 'youtube'
    && [DownloadStatusEnum.Downloading, DownloadStatusEnum.Verifying, DownloadStatusEnum.Complete].includes(item.status)
}

function youtubeStages(item: DownloadItem): Array<{ label: string; state: YouTubeStageState; icon: string }> {
  const current = (stageLabels.value[item.id] ?? '').toLowerCase()
  const isComplete = item.status === DownloadStatusEnum.Complete
  const isMerging = current.includes('mescl') || item.status === DownloadStatusEnum.Verifying
  const isAudio = current.includes('áudio') || current.includes('audio')

  const stateFor = (index: number): YouTubeStageState => {
    if (isComplete) return 'done'
    if (isMerging) return index < 2 ? 'done' : 'current'
    if (isAudio) return index === 0 ? 'done' : index === 1 ? 'current' : 'pending'
    return index === 0 ? 'current' : 'pending'
  }

  return ['Baixando vídeo', 'Baixando áudio', 'Mesclando'].map((label, index) => {
    const state = stateFor(index)
    return {
      label,
      state,
      icon: state === 'done'
        ? 'pi pi-check'
        : state === 'current'
          ? 'pi pi-spin pi-spinner'
          : 'pi pi-circle',
    }
  })
}

function isWaitingRetryNow(item: DownloadItem): boolean {
  return isWaitingRetry(item, nowTick.value)
}

function retryCountdownNow(item: DownloadItem): number {
  return retryCountdown(item, nowTick.value)
}

function statusTextValue(item: DownloadItem): string {
  if (item.status === DownloadStatusEnum.Downloading && stageLabels.value[item.id]) {
    return stageLabels.value[item.id]
  }
  return statusText(item, nowTick.value)
}

function sameYouTubeSelection(left: string, right: string): boolean {
  const leftFormat = youtubeFragmentValue(left, 'ytdlp_format')
  return leftFormat.length > 0 && leftFormat === youtubeFragmentValue(right, 'ytdlp_format')
}

function youtubeFragmentValue(url: string, key: string): string {
  const fragment = url.split('#')[1] ?? ''
  return new URLSearchParams(fragment).get(key) ?? ''
}

function statusColor(status: DownloadItem['status']): string {
  return STATUS_COLORS[status] ?? '#64748b'
}

function actionsFor(item: DownloadItem): Record<string, boolean> {
  return getDownloadActions(item)
}

function openCaptcha(id: string): void {
  activeCaptchaId.value = id
  const item = items.value.find((entry) => entry.id === id)
  if (item) {
    void openCaptchaWindow(item)
  }
}

function closeCaptchaModal(): void {
  activeCaptchaId.value = null
}

function onCaptchaDialogKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    closeCaptchaModal()
    return
  }

  trapDialogTab(event, captchaModalRef.value)
}

function reopenCaptchaWindow(): void {
  if (!activeCaptchaItem.value) {
    return
  }
  void openCaptchaWindow(activeCaptchaItem.value)
}

function isExpanded(id: string): boolean {
  return !!expandedFolders.value[id]
}

function openFolder(filePath: string): void {
  window.api.showInFolder(filePath)
}

function childNodes(children?: DownloadChild[]): DerivedChildNode[] {
  if (!children?.length) {
    return []
  }
  const cached = downloadChildNodeCache.get(children)
  if (cached) {
    return cached
  }
  const nodes = flattenChildTree(buildChildTree(children))
  downloadChildNodeCache.set(children, nodes)
  return nodes
}

function isExtractableArchive(filePath: string): boolean {
  const lower = filePath.toLowerCase()
  return (
    lower.endsWith('.zip') ||
    lower.endsWith('.rar') ||
    lower.endsWith('.7z') ||
    lower.endsWith('.tar') ||
    lower.endsWith('.tar.gz') ||
    lower.endsWith('.tgz') ||
    lower.endsWith('.tar.bz2') ||
    lower.endsWith('.tbz2') ||
    lower.endsWith('.tar.xz') ||
    lower.endsWith('.txz') ||
    lower.endsWith('.tar.zst')
  )
}

// ── Display helpers ────────────────────────────────────────
function moduleLabel(moduleId: string): string {
  return modulesById.value[moduleId]?.name ?? moduleId
}

function getIcon(moduleId: string): ReturnType<typeof getProviderIcon> {
  return getProviderIcon(moduleId)
}

function getProgressColor(item: DownloadItem): string {
  if (item.status === 'error' || item.status === 'corrupted') return '#ef4444'
  if (item.status === 'disk_full') return '#ef4444'
  if (item.status === 'verifying') return 'linear-gradient(90deg, #38bdf8, #60a5fa)'
  if (item.status === 'complete') return 'linear-gradient(90deg, #22c55e, #4ade80)'
  if (item.status === 'cancelled') return '#666'
  if (isWaitingRetryNow(item)) return 'linear-gradient(90deg, #f59e0b, #fbbf24)'
  // Use provider color for active downloads
  const color = modulesById.value[item.moduleId]?.color ?? getProviderColor(item.moduleId)
  return `linear-gradient(90deg, ${color}, ${color}cc)`
}

function childPercent(child: Pick<DownloadChild, 'size' | 'bytesDownloaded'>): number {
  if (!child.size || child.size <= 0) return 0
  const bytes = child.bytesDownloaded ?? 0
  return Math.max(0, Math.min(100, Math.floor((bytes / child.size) * 100)))
}

async function openCaptchaWindow(item: DownloadItem): Promise<void> {
  if (captchaWindowBusy.value) {
    return
  }

  captchaWindowBusy.value = true
  try {
    const token = await window.api.captcha.openWindow({
      provider: item.moduleId,
      pageUrl: item.captchaPageUrl ?? item.url,
      sourceUrl: item.url,
    }).catch(() => null)

    if (!token) {
      return
    }

    if (token === BROWSER_SESSION_READY_TOKEN && item.moduleId === 'katfile') {
      await window.api.downloads.force(item.id).catch(() => null)
    } else {
      await window.api.captcha.submit(item.id, token).catch(() => null)
    }
    captchaSolvedIds.value = new Set([...captchaSolvedIds.value, item.id])
    if (activeCaptchaId.value === item.id) {
      activeCaptchaId.value = null
    }
  } finally {
    captchaWindowBusy.value = false
  }
}

async function maybeResolveCaptcha(item: DownloadItem): Promise<void> {
  if (item.status !== DownloadStatusEnum.WaitingCaptcha || !item.captchaSitekey || captchaAttemptedIds.has(item.id)) {
    return
  }

  captchaAttemptedIds.add(item.id)
  const token = await window.api.captcha.nopechaSolve({
    type: item.captchaType ?? 'recaptcha2',
    sitekey: item.captchaSitekey,
    pageurl: item.captchaPageUrl ?? item.url,
  }).catch(() => null)

  if (token) {
    await window.api.captcha.submit(item.id, token).catch(() => null)
    captchaSolvedIds.value = new Set([...captchaSolvedIds.value, item.id])
    return
  }

  if (!activeCaptchaId.value) {
    activeCaptchaId.value = item.id
    void openCaptchaWindow(item)
  }
}

async function maybeResolveCaptchaById(id: string): Promise<void> {
  const item = items.value.find((entry) => entry.id === id)
  if (!item) {
    return
  }
  await maybeResolveCaptcha(item)
}
</script>

<style scoped>
/* ── Container ──────────────────────────────────────────────── */
.download-list {
  --row-height: 56px;
  display: flex;
  flex-direction: row;
  flex: 1;
  width: 100%;
  min-width: 0;
  min-height: 0;
  gap: 0;
  align-self: stretch;
  overflow: hidden;
}

.download-list.density-compact {
  --row-height: 40px;
}

.download-list.density-dense {
  --row-height: 28px;
}

.package-sidebar {
  width: 220px;
  min-width: 190px;
  max-width: 260px;
  flex: 0 0 220px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 2px 12px 2px 0;
  border-right: 1px solid var(--border-color);
  margin-right: 12px;
  overflow-y: auto;
}

.package-tree {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.package-node {
  display: grid;
  grid-template-columns: 10px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 34px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  text-align: left;
}

.package-node:hover,
.package-node.active {
  border-color: color-mix(in srgb, var(--accent-color) 22%, transparent);
  background: color-mix(in srgb, var(--accent-color) 8%, transparent);
  color: var(--text-primary);
}

.package-node-label {
  overflow: visible;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}

.package-node-count {
  min-width: 22px;
  padding: 2px 6px;
  border-radius: 999px;
  background: var(--bg-card);
  color: var(--text-muted);
  font-size: 11px;
  text-align: center;
}

.package-color-dot,
.package-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
}

.package-status-dot.ok { background: #22c55e; }
.package-status-dot.active { background: #3b82f6; }
.package-status-dot.failed { background: #ef4444; }
.package-status-dot.idle { background: #9ca3af; }

.columns-menu-wrap {
  position: relative;
}

/* ── Popover de exibição (densidade + campos + animação) ──────── */
.display-menu-wrap {
  position: relative;
}

.toolbar-btn.active {
  color: var(--accent-color);
  border-color: var(--accent-color);
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
}

.display-menu {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 30;
  width: 240px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-card);
  box-shadow: 0 16px 32px rgba(0, 0, 0, 0.22);
}

.display-menu-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.display-menu-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-muted);
}

.display-menu .density-toggle {
  width: 100%;
  height: 32px;
}

.display-menu .density-btn {
  flex: 1;
  width: auto;
}

.display-fields {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px 10px;
}

.display-field {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12.5px;
  color: var(--text-primary);
  cursor: pointer;
  user-select: none;
}

.display-field input[type='checkbox'] {
  accent-color: var(--accent-color);
  cursor: pointer;
  margin: 0;
}

.display-toggle-row {
  font-size: 13px;
}

.columns-menu {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 20;
  width: 190px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  box-shadow: 0 14px 28px rgba(0, 0, 0, 0.18);
}

.column-option {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 7px;
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 12px;
  cursor: grab;
}

.column-option:hover {
  background: color-mix(in srgb, var(--accent-color) 9%, transparent);
}

@media (max-width: 760px) {
  .download-list {
    flex-direction: column;
  }

  .package-sidebar {
    width: 100%;
    max-width: none;
    flex: 0 0 auto;
    flex-direction: row;
    overflow-x: auto;
    overflow-y: hidden;
    border-right: none;
    border-bottom: 1px solid var(--border-color);
    margin-right: 0;
    margin-bottom: 10px;
    padding: 0 0 8px;
  }

  .package-tree {
    flex-direction: row;
  }

  .package-node {
    min-width: 150px;
  }

  .queue-preview-panel {
    width: 100%;
    min-width: 0;
    flex: 0 0 auto;
    margin-left: 0;
    padding-left: 0;
    border-left: none;
    border-top: 1px solid var(--border-color);
    padding-top: 10px;
  }
}

/* ── Empty state ────────────────────────────────────────────── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  flex: 1;
  min-height: 0;
  gap: 10px;
  color: var(--text-muted);
  text-align: center;
}

.empty-icon {
  opacity: 0.4;
  margin-bottom: 8px;
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
  color: var(--text-muted);
}

.empty-sub {
  font-size: 13px;
  margin: 0;
  color: var(--text-muted);
  opacity: 0.7;
}

.empty-actions {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 8px;
}

.empty-primary,
.empty-secondary {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 34px;
  padding: 0 12px;
  border-radius: 7px;
  font-size: 12px;
  cursor: pointer;
}

.empty-primary {
  border: 1px solid var(--accent-color);
  background: var(--accent-color);
  color: white;
}

.empty-secondary {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
}

/* ── Items list ─────────────────────────────────────────────── */
.items-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  min-width: 0;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  gap: 10px;
  padding-right: 2px;
}

.list-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  min-width: 0;
}

.list-count {
  font-size: 12px;
  color: var(--text-muted);
}

.toolbar-actions {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.toolbar-sort {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-muted);
}

.toolbar-select {
  border: 1px solid var(--border-color);
  background: #ffffff;
  color: #111827;
  border-radius: 10px;
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 600;
  outline: none;
  color-scheme: light;
}

.toolbar-select option {
  background: #ffffff;
  color: #111827;
}

.toolbar-btn {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 10px;
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.density-toggle {
  display: inline-flex;
  height: 34px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  overflow: hidden;
  background: var(--bg-card);
}

.density-btn {
  width: 34px;
  border: 0;
  border-right: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.density-btn:last-child {
  border-right: 0;
}

.density-btn.active {
  color: var(--accent-color);
  background: color-mix(in srgb, var(--accent-color) 12%, transparent);
}

.queue-preview-panel {
  position: relative;
  width: 260px;
  min-width: 220px;
  flex: 0 0 260px;
  margin-left: 12px;
  padding-left: 12px;
  border-left: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  gap: 14px;
  overflow-y: auto;
}

.queue-panel-collapsed .queue-preview-panel {
  width: 24px;
  min-width: 24px;
  flex-basis: 24px;
  padding-left: 8px;
}

.queue-panel-handle {
  position: sticky;
  top: 0;
  z-index: 2;
  width: 24px;
  height: 42px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  color: var(--text-muted);
  cursor: pointer;
}

.queue-panel-section {
  display: flex;
  flex-direction: column;
  gap: 7px;
}

.queue-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
}

.mini-action {
  border: 1px solid color-mix(in srgb, var(--accent-color) 35%, var(--border-color));
  border-radius: 7px;
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
  color: var(--accent-color);
  font-size: 11px;
  padding: 5px 7px;
  cursor: pointer;
}

.mini-action:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.queue-empty {
  color: var(--text-muted);
  font-size: 12px;
  padding: 8px 0;
}

.queue-mini-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-height: 46px;
  padding: 8px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  color: var(--text-primary);
  text-align: left;
  cursor: grab;
}

.queue-mini-row span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-weight: 600;
}

.queue-mini-row em {
  color: var(--text-muted);
  font-size: 11px;
  font-style: normal;
}

.queue-mini-row.limited {
  border-color: rgba(245, 158, 11, 0.35);
}

.toolbar-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.items-stack {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
  min-width: 0;
  align-self: stretch;
}

/* Wrapper do TransitionGroup: display:contents mantém os cards como filhos
   diretos do flex (preserva o layout e a virtualização). */
.items-stack-rows {
  display: contents;
}

/* Reordenação suave (FLIP do Vue) — habilitada só fora da virtualização. */
.items-stack-rows.reorder-animate .reorder-move {
  transition: transform 0.32s cubic-bezier(0.22, 0.61, 0.36, 1);
  z-index: 1;
}

/* ── Download card ──────────────────────────────────────────── */
.download-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px;
  min-height: var(--row-height);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  position: relative;
  overflow: hidden;
  width: 100%;
  box-sizing: border-box;
  align-self: stretch;
  content-visibility: auto;
  contain: layout style;
}

.density-compact .download-card {
  padding: 10px 12px;
  gap: 10px;
}

.density-dense .download-card {
  padding: 7px 10px;
  gap: 8px;
}

.download-card::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3px;
  border-radius: 12px 0 0 12px;
  background: var(--status-indicator, var(--border-color));
}

.download-card.status-bg-downloading::before {
  background: var(--accent-gradient);
  animation: pulse-glow 2s ease-in-out infinite;
}

.download-card.status-bg-verifying::before {
  background: linear-gradient(180deg, #38bdf8, #60a5fa);
  animation: pulse-glow 2s ease-in-out infinite;
}

.download-card.status-bg-complete::before {
  background: linear-gradient(180deg, #22c55e, #4ade80);
}

.download-card.status-bg-error::before,
.download-card.status-bg-corrupted::before {
  background: #ef4444;
}

.download-card.status-bg-cancelled::before {
  background: #555;
}

.download-card:hover {
  border-color: color-mix(in srgb, var(--accent-color) 40%, var(--border-color));
  box-shadow: var(--shadow-card);
}

.download-card.selected {
  border-color: color-mix(in srgb, var(--accent-color) 70%, var(--border-color));
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-color) 28%, transparent);
}

.selection-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  height: 22px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--accent-color);
  color: white;
  font-size: 11px;
  font-weight: 800;
}

.download-context-menu {
  position: fixed;
  z-index: 80;
  width: 248px;
  max-height: min(620px, calc(100vh - 20px));
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 8px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  box-shadow: 0 18px 44px rgba(0, 0, 0, 0.28);
}

.context-menu-title {
  padding: 6px 8px 8px;
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 700;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
  margin-bottom: 3px;
}

.download-context-menu button {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 30px;
  padding: 0 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
  text-align: left;
}

.download-context-menu button:hover {
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
}

.download-context-menu button.danger {
  color: #ef4444;
}

.context-menu-group {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-top: 5px;
  margin-top: 4px;
  border-top: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
}

.context-menu-group > span {
  padding: 3px 8px;
  color: var(--text-muted);
  font-size: 10px;
  font-weight: 800;
  text-transform: uppercase;
}

/* ── Provider icon ──────────────────────────────────────────── */
.provider-icon {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, currentColor 28%, transparent);
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 2px;
}

.provider-icon :deep(svg) {
  width: 36px;
  height: 36px;
}

.provider-icon-thumb {
  padding: 0;
  border-color: rgba(255,255,255,0.1);
}

.item-thumb-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.item-channel {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--text-muted);
  margin-top: -2px;
}

.item-channel-avatar {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  object-fit: cover;
  display: block;
}

.youtube-stage-strip {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  min-height: 24px;
}

.youtube-stage {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--border-color);
  border-radius: 999px;
  color: var(--text-muted);
  background: color-mix(in srgb, var(--bg-card) 82%, var(--bg-primary));
  font-size: 11px;
  font-weight: 700;
}

.youtube-stage.done {
  border-color: color-mix(in srgb, #22c55e 42%, var(--border-color));
  color: #22c55e;
  background: color-mix(in srgb, #22c55e 10%, transparent);
}

.youtube-stage.current {
  border-color: color-mix(in srgb, var(--accent-color) 58%, var(--border-color));
  color: var(--text-primary);
  background: color-mix(in srgb, var(--accent-color) 14%, transparent);
}

.youtube-stage.pending {
  opacity: 0.72;
}

/* ── Item body ──────────────────────────────────────────────── */
.item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 7px;
}

/* ── Header row ─────────────────────────────────────────────── */
.item-header {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.item-title-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.item-title {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
}

.type-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  display: inline-block;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

.item-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

/* ── Status badge ───────────────────────────────────────────── */
.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.3px;
  text-transform: uppercase;
}

.status-detail-value {
  font-family: 'JetBrains Mono', 'Courier New', monospace;
}

.badge-pending {
  background: rgba(136, 136, 170, 0.15);
  color: var(--status-pending);
}

.badge-downloading {
  background: rgba(124, 111, 255, 0.15);
  color: var(--status-downloading);
}

.badge-verifying {
  background: rgba(56, 189, 248, 0.15);
  color: #38bdf8;
}

.badge-complete {
  background: rgba(34, 197, 94, 0.15);
  color: var(--status-complete);
}

.badge-error,
.badge-corrupted {
  background: rgba(239, 68, 68, 0.15);
  color: var(--status-error);
}

.badge-cancelled {
  background: rgba(100, 100, 120, 0.15);
  color: var(--status-cancelled);
}

.badge-paused {
  background: rgba(250, 204, 21, 0.15);
  color: var(--status-paused);
}

.badge-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-pending { background: var(--status-pending); }
.dot-downloading {
  background: var(--status-downloading);
  animation: pulse-glow 1.2s ease-in-out infinite;
}
.dot-verifying {
  background: #38bdf8;
  animation: pulse-glow 1.2s ease-in-out infinite;
}
.dot-complete { background: var(--status-complete); }
.dot-error,
.dot-corrupted { background: var(--status-error); }
.dot-cancelled { background: var(--status-cancelled); }
.dot-paused { background: var(--status-paused); }
.dot-rate_limited { background: #f59e0b; animation: pulse-glow 1.5s ease-in-out infinite; }
.dot-waiting_captcha { background: #8b5cf6; animation: pulse-glow 1.5s ease-in-out infinite; }

.badge-rate_limited {
  background: rgba(245, 158, 11, 0.15);
  color: #f59e0b;
}

.badge-waiting_captcha {
  background: rgba(139, 92, 246, 0.15);
  color: #8b5cf6;
}

.download-card.status-bg-rate_limited::before {
  background: linear-gradient(180deg, #f59e0b, #fbbf24);
  animation: pulse-glow 2s ease-in-out infinite;
}

.download-card.status-bg-waiting_captcha::before {
  background: linear-gradient(180deg, #8b5cf6, #a78bfa);
  animation: pulse-glow 2s ease-in-out infinite;
}

/* ── Action buttons ─────────────────────────────────────────── */
.cancel-btn,
.open-btn,
.action-btn {
  width: 26px;
  height: 26px;
  border-radius: 6px;
  border: 1px solid var(--border-color);
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 11px;
  transition: all 0.15s ease;
}

.action-btn {
  color: var(--text-muted);
}

.action-btn:hover {
  background: rgba(99, 102, 241, 0.15);
  border-color: #6366f1;
  color: #6366f1;
}

.cancel-btn {
  color: var(--text-muted);
}

.cancel-btn:hover {
  background: rgba(239, 68, 68, 0.15);
  border-color: #ef4444;
  color: #ef4444;
}

.open-btn {
  color: var(--text-muted);
}

.open-btn:hover {
  background: rgba(34, 197, 94, 0.15);
  border-color: #22c55e;
  color: #22c55e;
}

/* ── Progress track ─────────────────────────────────────────── */
.progress-track {
  height: 6px;
  background: var(--surface-section);
  border-radius: 999px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  border-radius: 999px;
  transition: width 0.55s ease-out;
  min-width: 2px;
  will-change: width;
}

.progress-shimmer {
  background-size: 200% 100% !important;
  animation: shimmer 1.8s ease-in-out infinite;
}

.row-rich-indicators {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 22px;
  flex-wrap: wrap;
}

.row-sparkline {
  width: 90px;
  height: 22px;
  flex: 0 0 auto;
  color: var(--accent-color);
  border: 1px solid color-mix(in srgb, var(--accent-color) 18%, transparent);
  border-radius: 6px;
  background: color-mix(in srgb, var(--accent-color) 5%, transparent);
}

.row-rich-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 7px;
  border-radius: 999px;
  border: 1px solid transparent;
  font-size: 10px;
  font-weight: 800;
  line-height: 1;
}

.row-rich-badge.tor {
  color: #7c3aed;
  border-color: rgba(124, 58, 237, 0.25);
  background: rgba(124, 58, 237, 0.1);
}

.row-tor-icon {
  width: 13px;
  height: 13px;
  flex: 0 0 auto;
  display: inline-flex;
}

.row-tor-icon :deep(svg) {
  width: 100%;
  height: 100%;
  display: block;
}

/* ── Chip discreto "Contornando limite via Tor" ───────────────── */
.tor-limit-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin-top: 6px;
  padding: 3px 8px 3px 6px;
  border-radius: 20px;
  background: var(--bg-secondary, rgba(0, 0, 0, 0.06));
  border: 1px solid color-mix(in srgb, #7d4698 22%, transparent);
}

.tor-limit-chip .row-tor-icon {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
  color: #7d4698;
  opacity: 0.85;
}

.tor-chip-text {
  font-size: 11px;
  color: var(--text-secondary, var(--text-muted));
  line-height: 1.2;
  white-space: nowrap;
}

.tor-chip-off {
  flex-shrink: 0;
  background: none;
  border: none;
  padding: 0 0 0 4px;
  font-size: 11px;
  color: var(--text-muted);
  cursor: pointer;
  text-decoration: underline;
  text-decoration-color: transparent;
  transition: color 0.15s, text-decoration-color 0.15s;
}

.tor-chip-off:hover {
  color: var(--text-secondary, var(--text-muted));
  text-decoration-color: currentColor;
}

.ctx-tor-icon {
  width: 14px;
  height: 14px;
  display: inline-flex;
  color: #7d4698;
}

.ctx-tor-icon :deep(svg) {
  width: 100%;
  height: 100%;
  display: block;
}

.row-rich-badge.premium {
  color: #2563eb;
  border-color: rgba(37, 99, 235, 0.24);
  background: rgba(37, 99, 235, 0.1);
}

.row-rich-badge.captcha {
  color: #8b5cf6;
  border-color: rgba(139, 92, 246, 0.24);
  background: rgba(139, 92, 246, 0.1);
}

.row-rich-badge.captcha-ok {
  color: #16a34a;
  border-color: rgba(22, 163, 74, 0.24);
  background: rgba(22, 163, 74, 0.1);
}

.row-rich-badge.limited {
  color: #d97706;
  border-color: rgba(217, 119, 6, 0.26);
  background: rgba(245, 158, 11, 0.12);
}

.row-rich-badge.verified {
  color: #0891b2;
  border-color: rgba(8, 145, 178, 0.24);
  background: rgba(8, 145, 178, 0.1);
}

.row-rich-badge.sequential,
.row-rich-badge.parts {
  color: var(--text-secondary);
  border-color: color-mix(in srgb, var(--border-color) 78%, transparent);
  background: color-mix(in srgb, var(--bg-primary) 65%, var(--bg-card));
}

.download-card:hover {
  z-index: 14;
}

@keyframes shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* ── Meta info ──────────────────────────────────────────────── */
.item-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  min-height: 24px;
  font-size: 11px;
  color: var(--text-muted);
}

.meta-percent {
  font-weight: 700;
  color: var(--text-primary);
  min-width: 32px;
}

.meta-sep {
  opacity: 0.4;
  padding: 0 2px;
}

.meta-speed {
  color: var(--accent-color);
  font-weight: 600;
}

.meta-eta {
  color: var(--text-muted);
}

.meta-wait,
.meta-wait-reason {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #f59e0b;
}

.meta-wait {
  font-weight: 600;
}

.meta-verifying {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #38bdf8;
  font-weight: 600;
}

.meta-size {
  color: var(--text-secondary);
  font-weight: 500;
  font-family: 'Courier New', monospace;
  font-size: 10.5px;
}

.meta-error {
  color: #fca5a5;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 200px;
}

.meta-retries {
  color: var(--text-muted);
  font-size: 10.5px;
}

/* ── Output path ────────────────────────────────────────────── */
.item-path {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 10.5px;
  color: var(--text-muted);
  font-family: 'Courier New', monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
  transition: color 0.15s;
  padding: 2px 0;
}

.item-path:hover {
  color: var(--accent-color);
}

.folder-children {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
  padding: 8px 10px;
  border: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--bg-card) 70%, transparent);
  overflow: hidden;
  transition: opacity 0.2s ease;
}

.detail-files-pane {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.folder-tree-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 82%, transparent);
  font-size: 11px;
  font-weight: 700;
  color: var(--text-secondary);
}

.child-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  color: var(--text-muted);
}

.download-child-virtual-list {
  width: 100%;
}

.download-child-virtual-list :deep(.virtual-row-shell) {
  padding-block: 3px;
}

.download-child-virtual-list :deep(.virtual-row-shell + .virtual-row-shell) {
  border-top: 1px solid color-mix(in srgb, var(--border-color) 72%, transparent);
}

.child-main {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.child-name {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.child-name-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.child-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  font-size: 10.5px;
  color: var(--text-muted);
}

.child-status {
  font-weight: 600;
  color: var(--text-primary);
}

.child-track {
  height: 4px;
  background: var(--surface-section);
  border-radius: 999px;
  overflow: hidden;
}

.child-fill {
  height: 100%;
  min-width: 2px;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--accent-color), color-mix(in srgb, var(--accent-color) 60%, #ffffff));
}

.child-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  display: inline-block;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

.child-size {
  flex-shrink: 0;
  color: var(--text-muted);
}

.child-folder-badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 7px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  color: #b27a00;
  background: rgba(255, 193, 7, 0.12);
}

.child-row.is-folder-row {
  background: color-mix(in srgb, var(--bg-card) 82%, rgba(255, 193, 7, 0.07));
  border-radius: 8px;
  padding: 4px 6px;
}

.download-detail-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 6px;
  padding: 10px;
  border: 1px solid color-mix(in srgb, var(--accent-color) 22%, var(--border-color));
  border-radius: 10px;
  background: color-mix(in srgb, var(--bg-primary) 88%, var(--bg-card));
  animation: detailSlide 0.18s ease-out;
}

@keyframes detailSlide {
  from {
    opacity: 0;
    transform: translateY(-4px);
    max-height: 0;
  }
  to {
    opacity: 1;
    transform: translateY(0);
    max-height: 420px;
  }
}

.detail-tabs {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.detail-tab {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-color);
  border-radius: 7px;
  background: var(--bg-card);
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 700;
  cursor: pointer;
}

.detail-tab.active {
  border-color: var(--accent-color);
  color: var(--accent-color);
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
}

.detail-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
  gap: 8px;
}

.detail-grid div,
.detail-action-pane,
.detail-event {
  min-width: 0;
  padding: 8px;
  border-radius: 8px;
  background: var(--bg-card);
}

.detail-grid span,
.detail-action-pane span,
.detail-event span {
  display: block;
  margin-bottom: 4px;
  color: var(--text-muted);
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
}

.detail-grid strong {
  display: block;
  overflow-wrap: anywhere;
  color: var(--text-primary);
  font-size: 11px;
  font-weight: 600;
}

.detail-wide {
  grid-column: 1 / -1;
}

.archive-password-editor {
  display: grid;
  grid-template-columns: minmax(120px, 0.28fr) minmax(240px, 1fr);
  align-items: center;
  gap: 8px 12px;
}

.archive-password-editor > span {
  margin-bottom: 0;
}

.archive-password-editor > div {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 0;
  background: transparent;
}

.archive-password-editor input {
  min-width: 0;
  flex: 1;
  height: 32px;
  border: 1px solid var(--border-color);
  border-radius: 7px;
  padding: 0 10px;
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: 12px;
  outline: none;
}

.archive-password-editor input:focus {
  border-color: var(--accent-color);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-color) 18%, transparent);
}

.archive-password-editor em {
  grid-column: 2;
  color: var(--text-muted);
  font-size: 11px;
  font-style: normal;
}

.detail-log-list,
.detail-events {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 240px;
  overflow: auto;
}

.detail-log-list p,
.detail-events p {
  margin: 0;
  color: var(--text-muted);
  font-size: 12px;
}

.detail-log-list code {
  display: block;
  padding: 7px 8px;
  border-radius: 7px;
  background: var(--bg-card);
  color: var(--text-secondary);
  font-size: 10.5px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.detail-action-pane {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.detail-event {
  display: grid;
  grid-template-columns: 120px 90px minmax(0, 1fr);
  gap: 8px;
  align-items: center;
  color: var(--text-secondary);
  font-size: 11px;
}

.detail-event span {
  margin: 0;
}

.detail-event strong {
  color: var(--accent-color);
  font-size: 11px;
}

.detail-event em {
  min-width: 0;
  overflow-wrap: anywhere;
  font-style: normal;
}

/* ── Captcha modal ──────────────────────────────────────────── */
.captcha-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(15, 23, 42, 0.42);
  backdrop-filter: blur(6px);
}

.captcha-modal {
  width: min(560px, 100%);
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 18px;
  border-radius: 18px;
  border: 1px solid color-mix(in srgb, #8b5cf6 24%, var(--border-color));
  background: var(--bg-card);
  box-shadow: 0 22px 50px rgba(15, 23, 42, 0.24);
}

.captcha-modal-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.captcha-modal-header strong {
  display: block;
  margin-bottom: 4px;
  font-size: 14px;
  color: var(--text-primary);
}

.captcha-modal-header p {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.captcha-modal-file {
  padding: 10px 12px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--bg-primary) 86%, var(--bg-secondary));
  color: var(--text-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.captcha-modal-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px;
  border-radius: 14px;
  background: color-mix(in srgb, var(--bg-primary) 88%, var(--bg-card));
  border: 1px solid color-mix(in srgb, var(--accent-primary, #5b6cff) 20%, var(--border-color));
}

.captcha-modal-body p {
  margin: 0;
  font-size: 13px;
  line-height: 1.55;
  color: var(--text-secondary);
}

.captcha-modal-open-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 42px;
  padding: 0 14px;
  border: none;
  border-radius: 12px;
  background: linear-gradient(135deg, var(--accent-primary, #5b6cff), color-mix(in srgb, var(--accent-primary, #5b6cff) 72%, #ffffff));
  color: white;
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
}

.captcha-modal-open-btn:disabled {
  opacity: 0.7;
  cursor: wait;
}

.captcha-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border: 1px solid color-mix(in srgb, #8b5cf6 30%, var(--border-color));
  border-radius: 10px;
  background: color-mix(in srgb, #8b5cf6 5%, var(--bg-card));
}

.captcha-header {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  font-weight: 600;
  color: #8b5cf6;
}

.captcha-frame {
  width: 100%;
  height: 82px;
  border: none;
  border-radius: 6px;
  background: transparent;
}

.meta-captcha-wait {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #8b5cf6;
  font-weight: 600;
}

.package-picker {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.package-picker select {
  max-width: 150px;
  height: 24px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 12px;
}

.package-dot {
  width: 8px;
  height: 8px;
  flex: 0 0 auto;
  border-radius: 999px;
}

@keyframes pulse-glow {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

/* ── Skeleton cards ─────────────────────────────────────────── */
@keyframes shimmer-skeleton {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}

.skeleton-card {
  pointer-events: none;
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px;
  min-height: var(--row-height);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  box-sizing: border-box;
  width: 100%;
}

.skeleton-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  flex-shrink: 0;
  background: linear-gradient(
    90deg,
    color-mix(in srgb, var(--text-muted) 12%, transparent) 25%,
    color-mix(in srgb, var(--text-muted) 28%, transparent) 50%,
    color-mix(in srgb, var(--text-muted) 12%, transparent) 75%
  );
  background-size: 200% 100%;
  animation: shimmer-skeleton 1.4s infinite;
}

.skeleton-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 9px;
  padding-top: 2px;
}

.skeleton-line {
  border-radius: 6px;
  background: linear-gradient(
    90deg,
    color-mix(in srgb, var(--text-muted) 12%, transparent) 25%,
    color-mix(in srgb, var(--text-muted) 28%, transparent) 50%,
    color-mix(in srgb, var(--text-muted) 12%, transparent) 75%
  );
  background-size: 200% 100%;
  animation: shimmer-skeleton 1.4s infinite;
}

.skeleton-title    { height: 13px; width: 60%; }
.skeleton-progress { height: 6px;  width: 100%; }
.skeleton-meta     { height: 10px; width: 35%; }

/* ── disk_full meta ─────────────────────────────────────────── */
.meta-disk-full {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #ef4444;
  font-weight: 600;
}

/* ── Status flash animation ─────────────────────────────────── */
@keyframes statusFlash {
  0% { background-color: color-mix(in srgb, var(--accent-color) 15%, transparent); }
  100% { background-color: transparent; }
}

.status-flash {
  animation: statusFlash 400ms ease-out;
}

/* ── Pin button ─────────────────────────────────────────────── */
.pin-btn {
  color: var(--text-muted);
}

.pin-btn-active {
  color: #fbbf24 !important;
  border-color: rgba(251, 191, 36, 0.4) !important;
  background: rgba(251, 191, 36, 0.1) !important;
}

.pin-btn:hover {
  color: #fbbf24 !important;
  border-color: rgba(251, 191, 36, 0.4) !important;
  background: rgba(251, 191, 36, 0.1) !important;
}

/* ── Pinned card indicator ──────────────────────────────────── */
.card-pinned {
  border-color: color-mix(in srgb, #fbbf24 30%, var(--border-color)) !important;
}

.card-pinned::after {
  content: '';
  position: absolute;
  top: 6px;
  right: 6px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #fbbf24;
  opacity: 0.7;
}

/* ── disk_full card indicator ──────────────────────────────── */
.download-card.status-bg-disk_full::before {
  background: #ef4444;
}

.badge-disk_full {
  background: rgba(239, 68, 68, 0.15);
  color: #ef4444;
}

.dot-disk_full {
  background: #ef4444;
}
</style>
