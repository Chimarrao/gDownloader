<template>
  <div class="tour-layer" role="dialog" aria-modal="true" aria-labelledby="tour-title">
    <div class="tour-dim"></div>
    <div
      v-if="targetRect"
      class="tour-spotlight"
      :style="spotlightStyle"
      aria-hidden="true"
    ></div>
    <div
      class="tour-card"
      :class="`placement-${placement}`"
      :style="cardStyle"
    >
      <div class="tour-kicker">
        <span>Passo {{ stepIndex + 1 }} de {{ steps.length }}</span>
        <button class="tour-skip" @click="finish">Pular</button>
      </div>
      <h2 id="tour-title">{{ currentStep.title }}</h2>
      <p>{{ currentStep.body }}</p>
      <div v-if="currentStep.checklist?.length" class="tour-checklist">
        <span v-for="item in currentStep.checklist" :key="item">
          <i class="pi pi-check"></i>
          {{ item }}
        </span>
      </div>
      <div class="tour-progress" aria-hidden="true">
        <span
          v-for="(_, index) in steps"
          :key="index"
          :class="{ active: index <= stepIndex }"
        ></span>
      </div>
      <div class="tour-actions">
        <button class="tour-secondary" :disabled="stepIndex === 0" @click="previous">
          Voltar
        </button>
        <button class="tour-secondary" @click="refreshTarget">
          Reposicionar
        </button>
        <button class="tour-primary" @click="next">
          {{ stepIndex === steps.length - 1 ? 'Concluir' : 'Próximo' }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'

type TourTab = 'downloads' | 'grabber' | 'settings' | 'account' | 'speed-history' | 'logs'

interface TourStep {
  tab: TourTab
  selector: string
  title: string
  body: string
  checklist?: string[]
}

const props = defineProps<{
  activeTab: TourTab
}>()

const emit = defineEmits<{
  (e: 'navigate', tab: TourTab): void
  (e: 'complete'): void
}>()

const steps: TourStep[] = [
  {
    tab: 'settings',
    selector: '[data-tour="download-folder"]',
    title: 'Escolha a pasta de download',
    body: 'Defina a pasta padrão antes de enfileirar arquivos. Ela vira o destino inicial para links capturados, containers e downloads interceptados pelo proxy local.',
    checklist: ['Use uma pasta com espaço livre', 'Você pode trocar por download quando precisar'],
  },
  {
    tab: 'grabber',
    selector: '[data-tour="link-input"]',
    title: 'Cole links no LinkGrabber',
    body: 'Cole uma URL ou uma lista inteira. O app detecta provider, tamanho, pastas e duplicatas antes de enviar para a fila.',
    checklist: ['Aceita vários links por linha', 'Containers DLC/CCF/RSDF entram pelo botão de importação'],
  },
  {
    tab: 'downloads',
    selector: '[data-tour="download-queue"]',
    title: 'Acompanhe e controle a fila',
    body: 'Aqui ficam progresso, pacotes, detalhes expansíveis, menu de contexto, rate-limit e o painel lateral com próximos itens.',
    checklist: ['Clique com o botão direito para ações rápidas', 'Use Q para alternar o painel lateral'],
  },
  {
    tab: 'settings',
    selector: '[data-tour="captcha-solver"]',
    title: 'Configure o resolvedor de captcha',
    body: 'Adicione sua chave NoPecha para resolver captchas automaticamente. Quando não houver token, o app abre a janela manual do próprio host.',
    checklist: ['A chave fica no SQLite local', 'Você pode deixar vazio e resolver manualmente'],
  },
  {
    tab: 'settings',
    selector: '[data-tour="browser-capture"]',
    title: 'Ative a captura local do navegador',
    body: 'A extensão antiga foi substituída pela captura local via proxy. Ative o proxy, instale a CA se for usar HTTPS e configure apps/navegador para 127.0.0.1:9667.',
    checklist: ['Tudo fica na rede local da máquina', 'Domínios sensíveis podem ficar na lista de ignorados'],
  },
]

const stepIndex = ref(0)
const targetRect = ref<DOMRect | null>(null)

const currentStep = computed(() => steps[stepIndex.value])
const spotlightStyle = computed(() => {
  const rect = targetRect.value
  if (!rect) return {}
  return {
    left: `${Math.max(8, rect.left - 8)}px`,
    top: `${Math.max(8, rect.top - 8)}px`,
    width: `${Math.min(window.innerWidth - 16, rect.width + 16)}px`,
    height: `${Math.min(window.innerHeight - 16, rect.height + 16)}px`,
  }
})

const cardLayout = computed(() => {
  const rect = targetRect.value
  const width = 380
  if (!rect) {
    return {
      placement: 'bottom',
      style: {
        left: `${Math.max(16, (window.innerWidth - width) / 2)}px`,
        top: '96px',
      },
    }
  }
  const gap = 16
  const belowSpace = window.innerHeight - rect.bottom
  const aboveSpace = rect.top
  const rightSpace = window.innerWidth - rect.right
  if (belowSpace > 240) {
    return {
      placement: 'bottom',
      style: {
        left: `${clamp(rect.left, 16, window.innerWidth - width - 16)}px`,
        top: `${rect.bottom + gap}px`,
      },
    }
  }
  if (aboveSpace > 240) {
    return {
      placement: 'top',
      style: {
        left: `${clamp(rect.left, 16, window.innerWidth - width - 16)}px`,
        top: `${Math.max(16, rect.top - 232)}px`,
      },
    }
  }
  if (rightSpace > width + gap) {
    return {
      placement: 'right',
      style: {
        left: `${rect.right + gap}px`,
        top: `${clamp(rect.top, 16, window.innerHeight - 232)}px`,
      },
    }
  }
  return {
    placement: 'left',
    style: {
      left: `${Math.max(16, rect.left - width - gap)}px`,
      top: `${clamp(rect.top, 16, window.innerHeight - 232)}px`,
    },
  }
})
const placement = computed(() => cardLayout.value.placement)
const cardStyle = computed(() => cardLayout.value.style)

watch(stepIndex, () => {
  void prepareStep()
})

watch(
  () => props.activeTab,
  () => {
    void refreshTarget()
  },
)

onMounted(() => {
  window.addEventListener('resize', refreshTarget)
  window.addEventListener('keydown', onKeydown)
  void prepareStep()
})

onUnmounted(() => {
  window.removeEventListener('resize', refreshTarget)
  window.removeEventListener('keydown', onKeydown)
})

async function prepareStep(): Promise<void> {
  const step = currentStep.value
  if (props.activeTab !== step.tab) {
    emit('navigate', step.tab)
  }
  await nextTick()
  await wait(90)
  await refreshTarget()
}

async function refreshTarget(): Promise<void> {
  await nextTick()
  const target = document.querySelector<HTMLElement>(currentStep.value.selector)
  if (!target) {
    targetRect.value = null
    return
  }
  target.scrollIntoView({ block: 'center', inline: 'center', behavior: 'smooth' })
  await wait(180)
  targetRect.value = target.getBoundingClientRect()
}

function previous(): void {
  stepIndex.value = Math.max(0, stepIndex.value - 1)
}

function next(): void {
  if (stepIndex.value >= steps.length - 1) {
    finish()
    return
  }
  stepIndex.value += 1
}

function finish(): void {
  emit('complete')
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    finish()
  } else if (event.key === 'ArrowRight') {
    next()
  } else if (event.key === 'ArrowLeft') {
    previous()
  }
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max))
}
</script>

<style scoped>
.tour-layer {
  position: fixed;
  inset: 0;
  z-index: 5000;
  pointer-events: none;
}

.tour-dim {
  position: absolute;
  inset: 0;
  background: rgba(8, 10, 18, 0.62);
  pointer-events: auto;
}

.tour-spotlight {
  position: fixed;
  border-radius: 12px;
  box-shadow:
    0 0 0 9999px rgba(8, 10, 18, 0.62),
    0 0 0 2px color-mix(in srgb, var(--accent-color) 75%, #fff),
    0 18px 50px rgba(0, 0, 0, 0.3);
  transition: all 0.2s ease;
  pointer-events: none;
}

.tour-card {
  position: fixed;
  z-index: 2;
  width: min(380px, calc(100vw - 32px));
  padding: 18px;
  border: 1px solid color-mix(in srgb, var(--accent-color) 28%, var(--border-color));
  border-radius: 12px;
  background: var(--bg-card);
  color: var(--text-primary);
  box-shadow: 0 24px 80px rgba(0, 0, 0, 0.32);
  pointer-events: auto;
}

.tour-kicker,
.tour-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.tour-kicker {
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 700;
}

.tour-skip {
  border: 0;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-weight: 700;
}

.tour-card h2 {
  margin: 12px 0 8px;
  font-size: 18px;
}

.tour-card p {
  margin: 0;
  color: var(--text-muted);
  font-size: 13px;
  line-height: 1.5;
}

.tour-checklist {
  display: grid;
  gap: 7px;
  margin-top: 14px;
}

.tour-checklist span {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
}

.tour-checklist i {
  color: #22c55e;
}

.tour-progress {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 5px;
  margin: 16px 0;
}

.tour-progress span {
  height: 4px;
  border-radius: 999px;
  background: var(--border-color);
}

.tour-progress span.active {
  background: var(--accent-color);
}

.tour-primary,
.tour-secondary {
  height: 34px;
  padding: 0 12px;
  border-radius: 8px;
  cursor: pointer;
  font-weight: 700;
}

.tour-primary {
  border: 1px solid var(--accent-color);
  background: var(--accent-color);
  color: #fff;
}

.tour-secondary {
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-primary);
}

.tour-secondary:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

@media (max-width: 720px) {
  .tour-card {
    left: 16px !important;
    right: 16px;
    top: auto !important;
    bottom: 16px;
  }
}
</style>
