<script setup lang="ts">
// The household's shared contacts (#20): vets, groomers, facilities — entered
// once, reused everywhere. Grouped by role; tap one to view it, then Edit (#44).
// Linking a contact to a specific pet happens on that pet's profile. Adding one
// is in the + speed-dial (#77), the one place for page actions.
import { ref, onMounted, computed } from 'vue'
import { api } from '../api'
import { usePageActions } from '../store'
import { contactRoleLabel } from '../format'
import { CONTACT_ROLES } from '../types'
import type { Contact } from '../types'
import ContactSheet from '../components/ContactSheet.vue'

const contacts = ref<Contact[]>([])
const loading = ref(true)
const error = ref('')
const openContact = ref<Contact | null>(null)
const adding = ref(false)
usePageActions(() => [{ key: 'add-contact', label: 'Add a contact', icon: 'contact', run: () => { adding.value = true } }])

// Groups in the canonical role order, skipping roles with no contacts.
const groups = computed(() =>
  CONTACT_ROLES
    .map((role) => ({ role, items: contacts.value.filter((c) => c.role === role) }))
    .filter((g) => g.items.length),
)

async function load() {
  loading.value = true
  error.value = ''
  try { contacts.value = (await api.contacts()) || [] } catch (e) { error.value = e.message } finally { loading.value = false }
}
onMounted(load)

function onSaved() { adding.value = false; openContact.value = null; load() }
function onDeleted() { openContact.value = null; load() }
</script>

<template>
  <div class="pad">
    <div class="section-h"><h3>Contacts</h3><span>{{ contacts.length }}</span></div>

    <div v-if="loading" class="loading">Loading…</div>
    <div v-else-if="error" class="errbox">{{ error }}</div>

    <template v-else>
      <div v-if="!contacts.length" class="card card-pad" style="text-align:center;color:var(--ink-2)">
        <p>No contacts yet. Use the + button to add your vet, groomer, or boarding facility once and link it to your pets.</p>
      </div>

      <template v-for="g in groups" :key="g.role">
        <div class="section-h"><h3>{{ contactRoleLabel(g.role) }}</h3><span>{{ g.items.length }}</span></div>
        <div class="card card-pad">
          <button
            v-for="c in g.items" :key="c.id" class="crow" data-testid="contact-row"
            @click="openContact = c"
          >
            <span class="role-badge" :data-role="c.role" aria-hidden="true">{{ c.name[0] }}</span>
            <span class="cbody">
              <b>{{ c.name }}</b>
              <small>
                <template v-if="c.doctor">{{ c.doctor }}</template>
                <template v-if="c.doctor && c.phone"> · </template>
                <span v-if="c.phone" class="mono">{{ c.phone }}</span>
                <template v-if="!c.doctor && !c.phone">{{ contactRoleLabel(c.role) }}</template>
                <template v-if="c.requirements && c.requirements.length"> · {{ c.requirements.length }} requirement{{ c.requirements.length === 1 ? '' : 's' }}</template>
              </small>
            </span>
            <svg class="cgo" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 6l6 6-6 6"/></svg>
          </button>
        </div>
      </template>
    </template>

    <ContactSheet v-if="adding" @close="adding = false" @saved="onSaved" @deleted="onDeleted" />
    <ContactSheet v-if="openContact" :contact="openContact" @close="openContact = null" @saved="onSaved" @deleted="onDeleted" />
  </div>
</template>

<style scoped>
.crow {
  display: flex; align-items: center; gap: 12px; width: 100%;
  padding: 12px 4px; background: none; border: none; cursor: pointer; text-align: left;
  border-bottom: 1px solid var(--border);
}
.crow:last-child { border-bottom: none; }
.crow:hover .cbody b { color: var(--brand); }
.crow:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; border-radius: 12px; }
.cbody { flex: 1 1 auto; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.cbody b { font-size: 15px; }
.cbody small { color: var(--ink-2); font-size: 12.5px; }
.cgo { flex: 0 0 18px; width: 18px; height: 18px; color: var(--ink-3); }
</style>
