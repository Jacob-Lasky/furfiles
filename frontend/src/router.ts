import { createRouter, createWebHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'
import Home from './views/Home.vue'
import Pet from './views/Pet.vue'
import Calendar from './views/Calendar.vue'
import Costs from './views/Costs.vue'
import Contacts from './views/Contacts.vue'
import Settings from './views/Settings.vue'
import Import from './views/Import.vue'

// Deep-linkable routes (#11). Axum serves index.html as an SPA fallback so these
// survive a hard refresh.
export const routes: RouteRecordRaw[] = [
  { path: '/', name: 'home', component: Home },
  { path: '/pets/:id', name: 'pet', component: Pet, props: true },
  { path: '/calendar', name: 'calendar', component: Calendar },
  { path: '/costs', name: 'costs', component: Costs },
  { path: '/contacts', name: 'contacts', component: Contacts },
  { path: '/settings', name: 'settings', component: Settings },
  // Bulk import (#23): one place to drop documents for any pets.
  { path: '/import', name: 'import', component: Import },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
  scrollBehavior: () => ({ top: 0 }),
})
