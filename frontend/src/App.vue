<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { api } from './api'
import { bumpData, pageActions } from './store'
import type { DialAction } from './store'
import type { User } from './types'
import Paw from './components/Paw.vue'
import AddRecordSheet from './components/AddRecordSheet.vue'
import SpeedDial from './components/SpeedDial.vue'

const router = useRouter()
const route = useRoute()
const users = ref<User[]>([])
const showAdd = ref(false)
const dark = ref(false)

// When you're already looking at a pet, a new record defaults to that pet. The
// :id param is a single string for this route (never a repeated/array param).
const currentPetId = computed(() => (route.name === 'pet' ? (route.params.id as string) : null))

// The speed-dial's actions (#77), listed top to bottom: what this page adds
// ("Edit Pedro", "Add a pet"), then the two that apply everywhere, with "Add a
// record" — the most common — last, right above the button. Import is dropped
// on the import page itself, where it would go nowhere.
const dialActions = computed<DialAction[]>(() => [
  ...pageActions.value,
  ...(route.path === '/import' ? [] : [{ key: 'import', label: 'Import documents', icon: 'import' as const, run: () => router.push('/import') }]),
  { key: 'add-record', label: 'Add a record', icon: 'record', run: () => { showAdd.value = true } },
])

function isDark() {
  const t = document.documentElement.getAttribute('data-theme')
  return t ? t === 'dark' : window.matchMedia('(prefers-color-scheme: dark)').matches
}
function toggleTheme() {
  const next = isDark() ? 'light' : 'dark'
  document.documentElement.setAttribute('data-theme', next)
  try { localStorage.setItem('furfiles-theme', next) } catch (_) { /* private mode */ }
  dark.value = isDark()
}

function onRecordCreated({ petId }: { petId: string }) {
  showAdd.value = false
  // A record added while on the same pet's profile lands on the same route
  // (no remount), so nudge the mounted view to reload its timeline.
  bumpData()
  router.push(`/pets/${petId}`)
}

onMounted(async () => {
  dark.value = isDark()
  try { users.value = await api.users() } catch (_) { /* non-fatal */ }
})
</script>

<template>
  <div class="app">
    <nav class="rail">
      <div class="rlogo"><span class="mark"><Paw /></span>Fur<span style="color:var(--brand)">Files</span></div>

      <!-- to="/" prefix-matches every route, so disable prefix-active and
           only light up on the exact home route. -->
      <router-link to="/" class="navb" active-class="nav-prefix" exact-active-class="router-link-active">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 12l9-8 9 8M5 10v10h14V10"/></svg>Home
      </router-link>
      <router-link to="/calendar" class="navb">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="17" rx="2"/><path d="M3 9h18M8 2v4M16 2v4"/></svg>Calendar
      </router-link>
      <router-link to="/costs" class="navb">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 3v18h18"/><path d="M7 15l4-5 3 3 5-7"/></svg>Costs
      </router-link>
      <router-link to="/import" class="navb" data-testid="nav-import">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M17 8l-5-5-5 5M12 3v12"/></svg>Import
      </router-link>
      <router-link to="/contacts" class="navb">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 00-3-3.87M16 3.13a4 4 0 010 7.75"/></svg>Contacts
      </router-link>
      <router-link to="/settings" class="navb">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 112.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>Settings
      </router-link>

      <div class="rail-foot">
        <div class="hh-avs">
          <span v-for="u in users" :key="u.id" class="av" :style="{ background: u.color }">{{ u.name[0] }}</span>
        </div>
        <div class="hh-meta"><b>Household</b><br /><small>{{ users.length }} member{{ users.length === 1 ? '' : 's' }}</small></div>
        <button class="theme-toggle" @click="toggleTheme" aria-label="Toggle theme" style="margin-left:auto">
          <svg v-if="dark" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4 12H2M22 12h-2M5 5l1.4 1.4M17.6 17.6L19 19M5 19l1.4-1.4M17.6 6.4L19 5"/></svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.8A9 9 0 1111.2 3a7 7 0 009.8 9.8z"/></svg>
        </button>
      </div>
    </nav>

    <main class="app-main">
      <router-view v-slot="{ Component }">
        <transition name="fade" mode="out-in">
          <component :is="Component" />
        </transition>
      </router-view>
    </main>

    <SpeedDial :actions="dialActions" />

    <AddRecordSheet v-if="showAdd" :default-pet-id="currentPetId" @close="showAdd = false" @created="onRecordCreated" />
  </div>
</template>
