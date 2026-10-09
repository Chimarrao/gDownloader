import { createApp } from 'vue'
import PrimeVue from 'primevue/config'
import Aura from '@primeuix/themes/aura'
import ToastService from 'primevue/toastservice'
import Tooltip from 'primevue/tooltip'
import App from './App.vue'
import { createWebApiShim } from './web-api-shim'
import 'primeicons/primeicons.css'
import 'flag-icons/css/flag-icons.min.css'
import './assets/main.css'

// Fora do Electron (acesso remoto pela LAN, direto no navegador) não existe
// contextBridge — window.api nunca é injetado. Preenche com o shim baseado em
// fetch/polling (ver web-api-shim.ts) ANTES de montar o app, pra todo
// componente que já chama window.api.* funcionar sem precisar saber se está
// rodando dentro do Electron ou num navegador remoto.
if (!window.api) {
  window.api = createWebApiShim()
}

const app = createApp(App)
app.use(PrimeVue, { theme: { preset: Aura } })
app.use(ToastService)
app.directive('tooltip', Tooltip)
app.mount('#app')
