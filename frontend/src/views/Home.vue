<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '../api'
import { usePageActions } from '../store'
import { andList, petGradient, dueLabel, isOverdue, isRehomed, longDate } from '../format'
import { COLLAGE_W, COLLAGE_H } from '../collage'
import type { Household, HouseholdPortraitInput, Pet, PetSummary } from '../types'
import PetCard from '../components/PetCard.vue'
import AddPetSheet from '../components/AddPetSheet.vue'
import FamilyPortraitSheet from '../components/FamilyPortraitSheet.vue'
import PhotoEditorSheet from '../components/PhotoEditorSheet.vue'
import Paw from '../components/Paw.vue'

const router = useRouter()
const pets = ref<PetSummary[]>([])
const loading = ref(true)
const error = ref('')
const showAddPet = ref(false)

// Family portrait (#75). Optional decoration: if it fails to load, Home simply
// shows the invitation, never an error.
const household = ref<Household | null>(null)
const portrait = computed(() => household.value?.portrait ?? null)
const portraitSheet = ref<'generate' | 'upload' | null>(null)
const removingPortrait = ref(false)
const portraitError = ref('')
const INVITE_KEY = 'furfiles-portrait-invite-dismissed'
function readDismissed() {
  try { return localStorage.getItem(INVITE_KEY) === '1' } catch (_) { return false }
}
const inviteDismissed = ref(readDismissed())
function dismissInvite() {
  inviteDismissed.value = true
  try { localStorage.setItem(INVITE_KEY, '1') } catch (_) { /* private mode */ }
}
// Describe who is actually IN a generated portrait (its pet_ids), not today's
// roster: after an archive the image still shows the pet who left.
const portraitAlt = computed(() => {
  const byId = new Map(pets.value.map((p) => [p.id, p.name]))
  const n = (portrait.value?.pet_ids || []).map((id) => byId.get(id)).filter((x): x is string => !!x)
  return n.length ? `Family portrait of ${andList(n)}` : 'Family portrait'
})
// Bumped by every local change, so a slow initial GET can't land after a save
// and put the old (or no) portrait back.
let portraitRev = 0
function setHousehold(h: Household) {
  portraitRev++
  household.value = h
}
function onPortraitSaved(h: Household) {
  setHousehold(h)
  portraitSheet.value = null
}
async function uploadPortrait(data: HouseholdPortraitInput) {
  setHousehold(await api.setHouseholdPortrait({ ...data, source: 'uploaded' }))
}
async function removePortrait() {
  if (!window.confirm('Remove the family portrait from Home?')) return
  removingPortrait.value = true
  portraitError.value = ''
  try {
    setHousehold(await api.deleteHouseholdPortrait())
  } catch (e) {
    portraitError.value = `Could not remove it: ${e.message}`
  } finally {
    removingPortrait.value = false
  }
}

// Adding a pet is a rare action (#14): a quiet trailing card in the grid, where
// the pets are, and an entry in the + speed-dial while on Home (#77) so the dial
// stays the one place every action can be found.
usePageActions(() => [{ key: 'add-pet', label: 'Add a pet', icon: 'pet', run: () => { showAddPet.value = true } }])
function onPetCreated(pet: Pet) {
  showAddPet.value = false
  router.push(`/pets/${pet.id}`)
}

const active = computed(() => pets.value.filter((p) => !p.archived_at))
// Archived pets split by HOW they left (#3): a pet that passed is remembered; a
// rehomed pet is alive, so it sits under a neutral heading with no memorial copy.
const memorial = computed(() => pets.value.filter((p) => p.archived_at && !isRehomed(p)))
const pastCompanions = computed(() => pets.value.filter((p) => isRehomed(p)))

// The soonest reminder across the household drives the "what's next" banner
// (#35): a neutral heads-up, never a grade. Pets with no reminder are simply
// caught up, so the banner goes calm when nothing is due.
const soonest = computed(() => {
  const withReminder = active.value.filter((p) => p.next_reminder)
  if (!withReminder.length) return null
  return withReminder.reduce((a, b) => (a.next_reminder!.due_on <= b.next_reminder!.due_on ? a : b))
})
const attention = computed(() => {
  const p = soonest.value
  if (p && p.next_reminder) {
    const r = p.next_reminder
    const label = dueLabel(r.due_on)
    return {
      calm: false,
      pet: p,
      title: `${p.name}: ${r.title}`,
      body: isOverdue(r.due_on)
        ? 'Overdue — open their profile to catch up and mark it done.'
        : `${label.charAt(0).toUpperCase()}${label.slice(1)} — open their profile to see what's next.`,
    }
  }
  const n = active.value.length
  return {
    calm: true,
    pet: null,
    title: 'Everyone is all caught up',
    body: `Nothing due across your ${n} pet${n === 1 ? '' : 's'} right now.`,
  }
})

onMounted(async () => {
  const loadPortrait = (async () => {
    const rev = portraitRev
    let h: Household | null = null
    try { h = await api.household() } catch (_) { /* optional: the invitation shows */ }
    if (rev === portraitRev) household.value = h
  })()
  try {
    pets.value = await api.pets(true)
  } catch (e) {
    error.value = e.message
  } finally {
    loading.value = false
  }
  await loadPortrait
})
</script>

<template>
  <div>
    <div class="apphead home-head">
      <div>
        <div class="eyebrow">Welcome home</div>
        <h1>The family</h1>
        <div class="sub">Everything you need to remember, in one warm place.</div>
      </div>

      <figure v-if="!loading && portrait" class="famport" data-testid="family-portrait" :data-source="portrait.source">
        <img :src="api.householdPortraitUrl(portrait)!" :alt="portraitAlt" :width="COLLAGE_W" :height="COLLAGE_H" />
        <figcaption>
          <span v-if="portrait.roster_changed" class="famport-stale" data-testid="portrait-stale">The family has changed since this was made</span>
          <span class="famport-acts">
            <button v-if="portrait.source === 'generated'" type="button" class="famport-btn" :class="{ hot: portrait.roster_changed }" :disabled="!active.length" data-testid="portrait-regenerate" @click="portraitSheet = 'generate'">Regenerate</button>
            <button type="button" class="famport-btn" data-testid="portrait-replace" @click="portraitSheet = 'upload'">{{ portrait.source === 'uploaded' ? 'Replace' : 'Use a photo' }}</button>
            <button type="button" class="famport-btn" :disabled="removingPortrait" data-testid="portrait-remove" @click="removePortrait">Remove</button>
          </span>
          <span v-if="portraitError" class="famport-err">{{ portraitError }}</span>
        </figcaption>
      </figure>

      <div v-else-if="!loading && !error && active.length && !inviteDismissed" class="famport-invite" data-testid="portrait-invite">
        <div class="famport-faces" aria-hidden="true">
          <span v-for="p in active.slice(0, 3)" :key="p.id" class="ff" :style="{ background: petGradient(p) }">
            <img v-if="api.petPhotoUrl(p)" :src="api.petPhotoUrl(p)!" alt="" />
            <template v-else>{{ p.name[0] }}</template>
          </span>
        </div>
        <div class="famport-invite-txt">
          <b>Make a family portrait</b>
          <small>From everyone's photos, or one of your own.</small>
          <span class="famport-acts">
            <button type="button" class="famport-btn hot" data-testid="portrait-invite-generate" @click="portraitSheet = 'generate'">Make one</button>
            <button type="button" class="famport-btn" data-testid="portrait-invite-upload" @click="portraitSheet = 'upload'">Upload a photo</button>
          </span>
        </div>
        <button type="button" class="famport-x" aria-label="Not now" data-testid="portrait-invite-dismiss" @click="dismissInvite">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.4"><path d="M6 6l12 12M18 6L6 18"/></svg>
        </button>
      </div>
    </div>

    <div v-if="loading" class="loading">Fetching your pets…</div>
    <div v-else-if="error" class="errbox">Could not load: {{ error }}</div>

    <template v-else>
      <router-link v-if="attention.pet" :to="`/pets/${attention.pet.id}`" class="attention">
        <h4>{{ attention.title }}</h4>
        <p>{{ attention.body }}</p>
        <span class="go">See {{ attention.pet.name }}
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2.4"><path d="M9 6l6 6-6 6"/></svg>
        </span>
      </router-link>
      <div v-else class="attention calm">
        <h4>{{ attention.title }}</h4>
        <p>{{ attention.body }}</p>
      </div>

      <div class="pad">
        <div class="section-h"><h3>Your pets</h3><span>{{ active.length }}</span></div>
        <div class="pgrid">
          <PetCard v-for="p in active" :key="p.id" :pet="p" />
          <button class="pcard padd" data-testid="add-pet-card" @click="showAddPet = true">
            <span class="padd-plus"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4"><path d="M12 5v14M5 12h14"/></svg></span>
            <span class="padd-txt">Add a pet</span>
            <Paw class="padd-paw" />
          </button>
        </div>
        <!-- The invitation can be waved off; this keeps the feature one quiet tap away. -->
        <button v-if="!portrait && inviteDismissed && active.length" type="button" class="famport-later" data-testid="portrait-later" @click="portraitSheet = 'generate'">
          Make a family portrait
        </button>

        <!-- Archived pets stay one tap away: their page and every record are kept
             and editable (#37). -->
        <template v-if="memorial.length">
          <div class="section-h"><h3>Remembering</h3></div>
          <router-link v-for="p in memorial" :key="p.id" :to="`/pets/${p.id}`" class="remember" data-testid="home-remember-card">
            <div class="rp" :style="{ background: petGradient(p) }">
              <img v-if="api.petPhotoUrl(p)" :src="api.petPhotoUrl(p)!" :alt="p.name" class="rp-photo" data-testid="home-archived-photo" />
              <span v-else class="init">{{ p.name[0] }}</span>
            </div>
            <div><h4>{{ p.name }}</h4><p>{{ p.memorial_note || 'Always remembered.' }}</p></div>
          </router-link>
        </template>

        <template v-if="pastCompanions.length">
          <div class="section-h"><h3>Past companions</h3></div>
          <router-link v-for="p in pastCompanions" :key="p.id" :to="`/pets/${p.id}`" class="remember moved" data-testid="home-past-card">
            <div class="rp" :style="{ background: petGradient(p) }">
              <img v-if="api.petPhotoUrl(p)" :src="api.petPhotoUrl(p)!" :alt="p.name" class="rp-photo" data-testid="home-archived-photo" />
              <span v-else class="init">{{ p.name[0] }}</span>
            </div>
            <div><h4>{{ p.name }}</h4><p>{{ p.memorial_note || (longDate(p.archived_at) ? `Moved on ${longDate(p.archived_at)}` : 'Moved on') }}</p></div>
          </router-link>
        </template>
      </div>
    </template>

    <AddPetSheet v-if="showAddPet" @close="showAddPet = false" @created="onPetCreated" />
    <FamilyPortraitSheet
      v-if="portraitSheet === 'generate'" :pets="active"
      @close="portraitSheet = null" @saved="onPortraitSaved" @upload="portraitSheet = 'upload'"
    />
    <PhotoEditorSheet
      v-if="portraitSheet === 'upload'" :width="COLLAGE_W" :height="COLLAGE_H"
      title="Family portrait" hint="Pick a photo of the crew, then drag and zoom to frame it."
      :upload="uploadPortrait" @close="portraitSheet = null" @saved="portraitSheet = null"
    />
  </div>
</template>
