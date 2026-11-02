import { createApp } from 'vue'
import App from './App.vue'
import { router } from './router'
import { api } from './api'
import { loadDisplay } from './store'
import './styles.css'

// Apply a previously chosen theme before mount so there's no flash of the
// wrong theme. With nothing saved, CSS falls back to the OS preference.
try {
  const saved = localStorage.getItem('furfiles-theme')
  if (saved) document.documentElement.setAttribute('data-theme', saved)
} catch (_) { /* private mode */ }

// Global capture (#41): report uncaught errors + unhandled promise rejections so
// a failure on a device I can't inspect still lands in the diagnostics store,
// stamped with the app version + user-agent server-side. Best-effort.
window.addEventListener('error', (e) => {
  if (!e.message && !e.error) return // ignore resource-load (img/script 404) errors
  api.reportDiag({ context: 'window.error', message: String(e.message || e.error), stack: e.error && e.error.stack })
})
window.addEventListener('unhandledrejection', (e) => {
  const r = e.reason
  api.reportDiag({ context: 'unhandledrejection', message: String((r && r.message) || r || 'rejection'), stack: r && r.stack })
})

// Household display preferences (#72); views render with the defaults until it lands.
loadDisplay()

createApp(App).use(router).mount('#app')
