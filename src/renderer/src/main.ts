import { createApp } from 'vue'
import PrimeVue from 'primevue/config'
import Aura from '@primeuix/themes/aura'
import ToastService from 'primevue/toastservice'
import Tooltip from 'primevue/tooltip'
import App from './App.vue'
import 'primeicons/primeicons.css'
import './assets/main.css'

const app = createApp(App)
app.use(PrimeVue, { theme: { preset: Aura } })
app.use(ToastService)
app.directive('tooltip', Tooltip)
app.mount('#app')
