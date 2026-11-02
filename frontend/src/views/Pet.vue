<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue'
import { api } from '../api'
import { dataVersion, display, usePageActions } from '../store'
import { petGradient, longDate, archiveHeading, isRehomed, ageFrom, sexAndStageTags, dueLabel, isOverdue, money, weightTrend, categoryColor, monthYear, fileSize, isImage, petMilestones, milestoneIcon, contactRoleLabel, insuranceKindLabel, regimenKindLabel, regimenKindIcon, regimenDose, protectionLabel } from '../format'
import type { Milestone } from '../format'
import type { PetDetail, RecordDetail, RecordLike, Reminder, Contact, InsurancePolicy, Regimen } from '../types'
import SpeciesMark from '../components/SpeciesMark.vue'
import RecordDetails from '../components/RecordDetails.vue'
import RecordSheet from '../components/RecordSheet.vue'
import ContactSheet from '../components/ContactSheet.vue'
import InsuranceSheet from '../components/InsuranceSheet.vue'
import PhotoEditorSheet from '../components/PhotoEditorSheet.vue'
import PetSheet from '../components/PetSheet.vue'
import RegimenSheet from '../components/RegimenSheet.vue'
import ArchiveSheet from '../components/ArchiveSheet.vue'
import SexSuggestionChip from '../components/SexSuggestionChip.vue'

// A timeline entry is either a (tappable) record or a derived milestone marker.
type TimelineItem =
  | { type: 'record'; date: string; record: RecordDetail }
  | { type: 'milestone'; date: string; milestone: Milestone }

const props = defineProps<{ id: string }>()
const data = ref<PetDetail | null>(null)
const loading = ref(true)
const error = ref('')
const openRecord = ref<RecordLike | null>(null) // the record open in the record sheet, or null
const showPhotoEditor = ref(false)
// Care team (#20): all contacts (for the "link existing" picker) + the sheets.
const allContacts = ref<Contact[]>([])
const addingContact = ref(false)
const openContact = ref<Contact | null>(null)
const linkPick = ref('') // contact id selected in the "link existing" dropdown
const contactError = ref('')
// Insurance & wellness plans (#8/#29): the add + view/edit sheets.
const addingInsurance = ref(false)
const openInsurance = ref<InsurancePolicy | null>(null)
// Profile edit (#63) — the only way to fill in microchip, colour, allergies and
// the dates on a pet that already exists. Opened from the speed-dial's "Edit
// <name>" (#77), the one place for actions; there is no in-page edit button.
const editingProfile = ref(false)
// Archive (#3): the "no longer with you" sheet, for archiving or editing it.
const archiving = ref(false)
const restoreError = ref('')
// Ongoing care (#63): the add + view/edit sheets.
const addingRegimen = ref(false)
const openRegimen = ref<Regimen | null>(null)

// Tap a timeline record to open it (#15, #44). The record sheet opens read-only
// first, with an explicit Edit inside — so a tap here is "view", not "edit". A
// click on a link inside the row (an attachment) is left to the browser.
function tapRecord(r: RecordLike, e?: Event) {
  if (e && e.target && (e.target as HTMLElement).closest('a')) return
  openRecord.value = r
}
function afterRecordChange() {
  openRecord.value = null
  load(props.id)
}

const pet = computed(() => data.value)

// An archived pet stays editable (#37), so the dial offers Edit on it too: the
// profile editor, same as for any pet. The archive itself (kind/date/note) is
// edited from the banner's own Edit, which sits next to what it changes.
// Offered only once the loaded pet IS the routed one: mid-navigation the old
// pet's data is still on screen, and editing it there would write onto the wrong pet.
usePageActions(() => (pet.value && pet.value.id === props.id
  ? [{ key: 'edit-pet', label: `Edit ${pet.value.name}`, icon: 'edit', run: () => { editingProfile.value = true } }]
  : []))
const photoUrl = computed(() => (pet.value ? api.petPhotoUrl(pet.value) : null))

async function onPhotoSaved() {
  showPhotoEditor.value = false
  await load(props.id)
}
const age = computed(() => (pet.value ? ageFrom(pet.value.birth_date) : null))
// Sex in the species' own words (#72) and life stage (#73), without the two
// repeating each other for horses.
const tags = computed(() => (pet.value ? sexAndStageTags(pet.value, display.breeder_terms) : { sex: null, stage: null }))
// The spay/neuter chip links to its record (#72).
function openRecordById(id: number) {
  openRecord.value = pet.value?.records.find((r) => r.id === id) ?? null
}
const homeSince = computed(() => (pet.value ? monthYear(pet.value.gotcha_date) : null))
const trend = computed(() => (pet.value ? weightTrend(pet.value.weights) : null))
const spend = computed(() => (pet.value ? pet.value.records.reduce((s, r) => s + (r.cost_cents || 0), 0) : 0))
const covered = computed(() => (pet.value ? pet.value.records.reduce((s, r) => s + (r.covered_cents || 0), 0) : 0))
const latestWeight = computed(() => {
  if (!pet.value || !pet.value.weights.length) return null
  return pet.value.weights[pet.value.weights.length - 1]
})
// Timeline = records + one-time milestones (born, came home, #32), newest first.
// Milestones are derived, non-editable markers; records stay tappable.
const timeline = computed<TimelineItem[]>(() => {
  if (!pet.value) return []
  const items: TimelineItem[] = pet.value.records.map((r) => ({ type: 'record', date: r.occurred_on, record: r }))
  for (const m of petMilestones(pet.value)) items.push({ type: 'milestone', date: m.date, milestone: m })
  return items.sort((a, b) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0))
})

// Insurance policies + wellness plans (#8/#29). The tile reflects the active
// insurance policy (falling back to the pet's stored insurer fields).
const policies = computed<InsurancePolicy[]>(() => pet.value?.insurance_policies ?? [])
const activePolicy = computed<InsurancePolicy | null>(() => {
  const ins = policies.value.filter((p) => p.kind === 'insurance')
  return ins.find((p) => p.status === 'active') || ins[0] || null
})
const insurerName = computed(() => activePolicy.value?.provider || pet.value?.insurer || null)
const insurerCap = computed(() => {
  const p = activePolicy.value
  if (p) return p.reimburse_percent != null ? `${p.reimburse_percent}% reimbursement` : 'policy on file'
  return pet.value?.insurer ? `${pet.value.insurer_reimburse} reimbursement` : 'all out of pocket'
})
function policyLine(p: InsurancePolicy): string {
  const bits: string[] = [insuranceKindLabel(p.kind)]
  if (p.reimburse_percent != null) bits.push(`${p.reimburse_percent}% back`)
  if (p.deductible_cents != null) bits.push(`${money(p.deductible_cents)} deductible`)
  if (p.status !== 'active') bits.push(p.status)
  return bits.join(' · ')
}

// Ongoing care (#63), split so the profile leads with what the pet is on NOW.
// "Current" is `ended_on` being empty — the same test a vet intake form asks.
const regimens = computed<Regimen[]>(() => pet.value?.regimens ?? [])
const currentCare = computed<Regimen[]>(() => regimens.value.filter((r) => !r.ended_on))
const pastCare = computed<Regimen[]>(() => regimens.value.filter((r) => !!r.ended_on))

// The pet's care team, and the contacts NOT yet on it (offered in the picker).
const careTeam = computed<Contact[]>(() => pet.value?.contacts ?? [])
const linkable = computed<Contact[]>(() => {
  const linked = new Set(careTeam.value.map((c) => c.id))
  return allContacts.value.filter((c) => !linked.has(c.id))
})

// Only the newest request may land: after A → B, a slow reply for A arriving
// last would otherwise put A's profile on B's route (and the speed-dial, which
// checks the ids, would then offer no Edit at all).
let loadSeq = 0
async function load(id: string) {
  const seq = ++loadSeq
  loading.value = true
  error.value = ''
  try {
    const d = await api.pet(id)
    if (seq === loadSeq) data.value = d
  } catch (e) {
    if (seq === loadSeq) error.value = e.message
  } finally {
    if (seq === loadSeq) loading.value = false
  }
}
async function loadContacts() {
  try { allContacts.value = (await api.contacts()) || [] } catch (_) { /* non-fatal */ }
}
async function toggleReminder(r: Reminder) {
  try {
    await api.updateReminder(r.id, { done: !r.done })
    await load(props.id)
  } catch (e) { error.value = e.message }
}

// Care team (#20): link an existing contact, unlink one, or add a new contact
// and link it in one step. Every change reloads the pet so the section + the
// record clinic picker stay in sync.
async function linkExisting() {
  if (!linkPick.value || !pet.value) return
  contactError.value = ''
  try {
    await api.linkPetContact(pet.value.id, linkPick.value)
    linkPick.value = ''
    await load(props.id)
  } catch (e) { contactError.value = e.message || 'Could not link that contact.' }
}
async function unlink(c: Contact) {
  if (!pet.value) return
  contactError.value = ''
  try {
    await api.unlinkPetContact(pet.value.id, c.id)
    await load(props.id)
  } catch (e) { contactError.value = e.message || 'Could not unlink that contact.' }
}
// A newly-created contact is linked to this pet straight away.
async function onContactAdded(c: Contact) {
  addingContact.value = false
  if (pet.value) {
    try { await api.linkPetContact(pet.value.id, c.id) } catch (e) { contactError.value = e.message }
  }
  await loadContacts()
  await load(props.id)
}
async function onContactSaved() {
  openContact.value = null
  await loadContacts()
  await load(props.id)
}
async function onContactDeleted() {
  openContact.value = null
  await loadContacts()
  await load(props.id)
}

// Insurance (#8/#29): every add/edit/delete reloads the pet so the section + the
// Insurance tile stay in sync (a saved policy may sync the pet's insurer fields).
async function onInsuranceSaved() {
  addingInsurance.value = false
  openInsurance.value = null
  await load(props.id)
}
async function onInsuranceDeleted() {
  openInsurance.value = null
  await load(props.id)
}

// Profile + ongoing care (#63): every change reloads the pet so the hero tags,
// the allergy alert and the care section all reflect it at once.
async function onProfileSaved() {
  editingProfile.value = false
  await load(props.id)
}
// PetSheet hands off to the ArchiveSheet; one sheet open at a time.
function startArchive() {
  editingProfile.value = false
  archiving.value = true
}
async function onArchived() {
  archiving.value = false
  await load(props.id)
}
// "Bring back" (#3): an archive must be undoable. Nothing else to restore —
// records, reminders and attachments were never touched by the archive.
async function restore() {
  restoreError.value = ''
  try {
    await api.unarchivePet(props.id)
    await load(props.id)
  } catch (e) {
    restoreError.value = (e as Error).message || 'Could not bring them back.'
  }
}
async function onRegimenSaved() {
  addingRegimen.value = false
  openRegimen.value = null
  await load(props.id)
}
async function onRegimenDeleted() {
  openRegimen.value = null
  await load(props.id)
}

onMounted(() => { load(props.id); loadContacts() })
// Navigating to another pet reuses this component, so any sheet left open would
// keep editing the PREVIOUS pet's data while the header says the new one — a
// save then wrote pet A's profile onto pet B, and a delete removed A's care
// entry under a confirmation naming B. Close every sheet on the way through.
watch(() => props.id, (id) => {
  editingProfile.value = false
  archiving.value = false
  restoreError.value = ''
  addingRegimen.value = false
  openRegimen.value = null
  openRecord.value = null
  openContact.value = null
  addingContact.value = false
  addingInsurance.value = false
  openInsurance.value = null
  showPhotoEditor.value = false
  load(id)
})
// A record added while viewing this same pet navigates to the same route (no
// remount); reload the timeline when the shared data signal bumps.
watch(dataVersion, () => load(props.id))
</script>

<template>
  <div>
    <router-link to="/" class="back-btn">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 18l-6-6 6-6"/></svg>The family
    </router-link>

    <div v-if="loading" class="loading">Loading…</div>
    <div v-else-if="error" class="errbox">{{ error }}</div>

    <template v-else-if="pet">
      <div class="phero">
        <div class="phero-bg" :style="{ background: petGradient(pet) }"></div>
        <button
          class="phero-portrait" :style="{ background: petGradient(pet) }"
          :aria-label="photoUrl ? `Change ${pet.name}'s photo` : `Add a photo of ${pet.name}`"
          @click="showPhotoEditor = true"
        >
          <img v-if="photoUrl" class="pphoto" :src="photoUrl" :alt="pet.name" />
          <template v-else>
            <span class="init">{{ pet.name[0] }}</span><SpeciesMark :species="pet.species" class="paw" />
          </template>
          <span class="photo-edit" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 7h3l2-3h6l2 3h3a1 1 0 011 1v11a1 1 0 01-1 1H4a1 1 0 01-1-1V8a1 1 0 011-1z"/><circle cx="12" cy="13" r="4"/></svg>
          </span>
        </button>
        <div class="phero-main">
          <h1 data-testid="pet-name">{{ pet.name }}</h1>
          <div class="breed">{{ pet.breed || pet.species }} · {{ pet.species }}</div>
          <div class="phero-tags">
            <span v-if="tags.sex" class="tag" data-testid="pet-sex-tag">{{ tags.sex }}</span>
            <span v-if="age" class="tag">{{ age }}</span>
            <span v-if="tags.stage" class="tag stage" data-testid="pet-life-stage">{{ tags.stage }}</span>
            <span v-if="pet.color" class="tag" data-testid="pet-color-tag">{{ pet.color }}</span>
            <span v-if="homeSince" class="tag">Home since {{ homeSince }}</span>
            <span v-if="pet.weight" class="tag">{{ pet.weight }} {{ pet.weight_unit }}</span>
            <span v-if="pet.microchip && pet.microchip !== '—'" class="tag mono">chip {{ pet.microchip }}</span>
          </div>
          <SexSuggestionChip :pet="pet" @updated="load(props.id)" @open-record="openRecordById" />
        </div>
        <!-- An archived pet's page stays fully viewable and editable (#37); the
             banner says what happened, in the right words for HOW it happened. -->
        <div v-if="pet.archived_at" class="archived-banner" data-testid="archived-banner" :data-kind="isRehomed(pet) ? 'rehomed' : 'passed'">
          <span class="ab-ico" aria-hidden="true">{{ isRehomed(pet) ? '🌿' : '🕊️' }}</span>
          <div class="ab-txt">
            <h4>{{ archiveHeading(pet) }}<span v-if="longDate(pet.archived_at)" class="ab-date"> · {{ longDate(pet.archived_at) }}</span></h4>
            <p v-if="pet.memorial_note" data-testid="archived-note">{{ pet.memorial_note }}</p>
            <div class="ab-acts">
              <button type="button" data-testid="archived-edit" @click="archiving = true">Edit</button>
              <button type="button" data-testid="archived-restore" @click="restore">Bring back to active</button>
            </div>
            <p v-if="restoreError" class="ab-err">{{ restoreError }}</p>
          </div>
        </div>
        <div v-else class="nextup" data-testid="next-up" :class="{ caught: !pet.next_reminder }">
          <span class="nextup-icon" aria-hidden="true">{{ pet.next_reminder ? '🔔' : '✓' }}</span>
          <div class="nextup-txt" v-if="pet.next_reminder">
            <h4>Next up</h4>
            <p><b>{{ pet.next_reminder.title }}</b><span class="nextup-due" :class="{ over: isOverdue(pet.next_reminder.due_on) }"> · {{ dueLabel(pet.next_reminder.due_on) }}</span></p>
          </div>
          <div class="nextup-txt" v-else>
            <h4>All caught up</h4>
            <p>Nothing due for {{ pet.name }} right now.</p>
          </div>
        </div>
      </div>

      <div class="pad">
        <!-- Allergies are a medical alert, not a detail: anyone looking after
             this pet needs them before anything else on the page (#63). -->
        <div v-if="pet.allergies && pet.allergies.length" class="allergy-alert" data-testid="allergy-alert">
          <span class="allergy-icon" aria-hidden="true">⚠</span>
          <div>
            <h4>Allergies</h4>
            <div class="allergy-chips">
              <span v-for="(a, i) in pet.allergies" :key="i" class="allergy-chip" :title="a.detail || ''">
                {{ a.label }}<template v-if="a.detail"> · {{ a.detail }}</template>
              </span>
            </div>
          </div>
        </div>

        <div class="tiles">
          <div class="tile"><div class="lab">Weight</div><div class="big mono">{{ latestWeight ? latestWeight.value : '—' }}<small v-if="latestWeight"> {{ latestWeight.unit }}</small></div><div class="cap">{{ latestWeight ? 'latest reading' : 'not tracked' }}</div></div>
          <div class="tile"><div class="lab">Insurance</div><div class="big" style="font-size:19px">{{ insurerName || 'None' }}</div><div class="cap">{{ insurerCap }}</div></div>
          <div class="tile"><div class="lab">Spent</div><div class="big mono">{{ money(spend) }}</div><div class="cap">{{ covered ? money(covered) + ' covered' : 'no claims' }}</div></div>
          <div class="tile"><div class="lab">Records</div><div class="big">{{ pet.records.length }}</div><div class="cap">on file</div></div>
        </div>

        <div class="section-h"><h3>Weight trend</h3></div>
        <div class="card card-pad">
          <div class="chart-head"><h4>Body weight</h4><div class="now mono" v-if="latestWeight">{{ latestWeight.value }}<small> {{ latestWeight.unit }}</small></div></div>
          <svg v-if="trend" class="trend" data-testid="weight-trend" :viewBox="`0 0 ${trend.w} ${trend.h}`" preserveAspectRatio="none">
            <defs><linearGradient id="tgrad" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="var(--brand)" stop-opacity=".22"/><stop offset="100%" stop-color="var(--brand)" stop-opacity="0"/></linearGradient></defs>
            <path :d="`${trend.line} L ${trend.last[0]},${trend.h} L 16,${trend.h} Z`" fill="url(#tgrad)" />
            <path :d="trend.line" fill="none" stroke="var(--brand)" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" />
            <circle :cx="trend.last[0]" :cy="trend.last[1]" r="4.5" fill="var(--brand)" stroke="var(--surface)" stroke-width="2.5" />
          </svg>
          <div v-else style="color:var(--ink-3);font-size:13px;padding:20px 0;text-align:center">Not enough weight history yet.</div>
        </div>

        <template v-if="pet.reminders && pet.reminders.length">
          <div class="section-h"><h3>Reminders</h3><span v-if="pet.archived_at" data-testid="reminders-paused">paused</span><span v-else>{{ pet.reminders.filter((r) => !r.done).length }} upcoming</span></div>
          <div class="card card-pad">
            <div v-for="r in pet.reminders" :key="r.id" class="rem" :class="{ done: r.done }">
              <button class="rem-check" @click="toggleReminder(r)" :aria-label="r.done ? 'Mark not done' : 'Mark done'">
                <svg v-if="r.done" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><path d="M20 6L9 17l-5-5"/></svg>
              </button>
              <div class="rem-body"><b>{{ r.title }}</b><small>{{ r.due_on }}</small></div>
              <span v-if="r.source === 'scan'" class="aichip" style="margin:0">scanned</span>
            </div>
          </div>
        </template>

        <!-- Ongoing care (#63): what this pet is ON — its food, preventatives,
             medications and supplements. One list rather than flat diet/flea/
             tick/heartworm fields, because one product covers several. -->
        <div class="section-h"><h3>Ongoing care</h3><span v-if="currentCare.length">{{ currentCare.length }} current</span></div>
        <div class="card card-pad" data-testid="care-section">
          <div v-if="!regimens.length" style="color:var(--ink-3);font-size:14px;padding:2px 0 12px">
            No food, preventatives or medications on file yet.
          </div>
          <div
            v-for="r in currentCare" :key="r.id" class="carerow" data-testid="care-regimen" :data-kind="r.kind"
            role="button" tabindex="0"
            @click="openRegimen = r" @keydown.enter="openRegimen = r" @keydown.space.prevent="openRegimen = r"
          >
            <span class="care-badge" :data-kind="r.kind" aria-hidden="true">{{ regimenKindIcon(r.kind) }}</span>
            <span class="carebody">
              <b>{{ r.product }}</b>
              <small>{{ regimenKindLabel(r.kind) }}<template v-if="regimenDose(r)"> · {{ regimenDose(r) }}</template></small>
              <!-- What a preventative covers: the intake-form answer. -->
              <span v-if="r.protects && r.protects.length" class="reqchips">
                <span v-for="p in r.protects" :key="p" class="reqchip prot">{{ protectionLabel(p) }}</span>
              </span>
            </span>
          </div>
          <!-- Things the pet is off: kept, but visually demoted. -->
          <template v-if="pastCare.length">
            <div class="care-past-h">No longer on</div>
            <div
              v-for="r in pastCare" :key="r.id" class="carerow past" data-testid="care-regimen-past"
              role="button" tabindex="0"
              @click="openRegimen = r" @keydown.enter="openRegimen = r" @keydown.space.prevent="openRegimen = r"
            >
              <span class="care-badge" :data-kind="r.kind" aria-hidden="true">{{ regimenKindIcon(r.kind) }}</span>
              <span class="carebody">
                <b>{{ r.product }}</b>
                <small>{{ regimenKindLabel(r.kind) }} · stopped {{ r.ended_on }}</small>
              </span>
            </div>
          </template>
          <button class="addrow" type="button" data-testid="add-regimen" @click="addingRegimen = true">+ Add food, medication or preventative</button>
        </div>

        <!-- Care team (#20): the shared vets/facilities linked to this pet. -->
        <div class="section-h"><h3>Care team</h3><span v-if="careTeam.length">{{ careTeam.length }}</span></div>
        <div class="card card-pad" data-testid="care-team">
          <div v-if="!careTeam.length" style="color:var(--ink-3);font-size:14px;padding:2px 0 12px">
            No vets or facilities linked yet.
          </div>
          <div
            v-for="c in careTeam" :key="c.id" class="ctrow" data-testid="care-contact" role="button" tabindex="0"
            @click="openContact = c" @keydown.enter="openContact = c" @keydown.space.prevent="openContact = c"
          >
            <span class="role-badge" :data-role="c.role" aria-hidden="true">{{ c.name[0] }}</span>
            <span class="ctbody">
              <b>{{ c.name }}</b>
              <small>{{ contactRoleLabel(c.role) }}<template v-if="c.doctor"> · {{ c.doctor }}</template><template v-if="c.phone"> · {{ c.phone }}</template></small>
              <!-- Hours (#42 follow-up): open + drop-off/pick-up windows. -->
              <span v-if="c.hours && c.hours.length" class="cthours">
                <span v-for="(h, i) in c.hours" :key="i" class="cthour"><b>{{ h.label }}</b><template v-if="h.detail"> {{ h.detail }}</template></span>
              </span>
              <!-- Facility requirements (#42): what to bring / have current for a stay. -->
              <span v-if="c.requirements && c.requirements.length" class="reqchips">
                <span v-for="(r, i) in c.requirements" :key="i" class="reqchip" :title="r.detail || ''">{{ r.label }}</span>
              </span>
            </span>
            <button class="fx" type="button" :aria-label="`Unlink ${c.name}`" @click.stop="unlink(c)">×</button>
          </div>
          <div class="ctadd">
            <select v-if="linkable.length" v-model="linkPick" class="ctlink" aria-label="Link an existing contact" @change="linkExisting">
              <option value="">Link an existing contact…</option>
              <option v-for="c in linkable" :key="c.id" :value="c.id">{{ c.name }} · {{ contactRoleLabel(c.role) }}</option>
            </select>
            <button class="addrow" type="button" data-testid="add-care-contact" @click="addingContact = true">+ New contact</button>
          </div>
          <p v-if="contactError" style="color:var(--crit);font-size:13px;margin:8px 0 0">{{ contactError }}</p>
        </div>

        <!-- Insurance & wellness plans (#8/#29): policies with their documents. -->
        <div class="section-h"><h3>Insurance</h3><span v-if="policies.length">{{ policies.length }}</span></div>
        <div class="card card-pad" data-testid="insurance-section">
          <div v-if="!policies.length" style="color:var(--ink-3);font-size:14px;padding:2px 0 12px">
            No policies or plans on file yet.
          </div>
          <div
            v-for="p in policies" :key="p.id" class="insrow" data-testid="insurance-policy-row" :data-policy="p.id" role="button" tabindex="0"
            @click="openInsurance = p" @keydown.enter="openInsurance = p" @keydown.space.prevent="openInsurance = p"
          >
            <span class="ins-badge" :data-kind="p.kind" aria-hidden="true">{{ p.provider[0] }}</span>
            <span class="insbody">
              <b>{{ p.provider }}<template v-if="p.plan_name"> · {{ p.plan_name }}</template></b>
              <small>{{ policyLine(p) }}</small>
            </span>
            <span v-if="p.document_name" class="ins-doc" aria-label="has document">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><path d="M14 2v6h6"/></svg>
            </span>
          </div>
          <button class="addrow" type="button" data-testid="add-insurance" @click="addingInsurance = true">+ Add insurance</button>
        </div>

        <div class="section-h"><h3>Timeline</h3><span>{{ pet.records.length }} records</span></div>
        <div class="card card-pad">
          <div v-if="!timeline.length" style="color:var(--ink-3);font-size:14px;padding:8px 0">No records yet.</div>
          <template v-for="it in timeline" :key="it.type === 'record' ? `r${it.record.id}` : `m${it.milestone.subkind}`">
            <!-- Milestone: a derived, non-editable life marker (#32). -->
            <div v-if="it.type === 'milestone'" class="tevent milestone" data-testid="timeline-milestone" :data-kind="it.milestone.subkind">
              <div class="rail-dot"><span class="dot" :style="{ background: 'var(--milestone)' }"></span></div>
              <div class="body">
                <h4><span class="ms-emoji" aria-hidden="true">{{ milestoneIcon(it.milestone.subkind) }}</span>{{ it.milestone.title }}</h4>
                <div class="m">{{ it.milestone.date }}</div>
              </div>
            </div>
            <!-- Record: tappable to edit (#15). -->
            <div v-else class="tevent clickable" role="button" tabindex="0" data-testid="timeline-event" :data-record="it.record.id"
                 @click="tapRecord(it.record, $event)" @keydown.enter="tapRecord(it.record)" @keydown.space.prevent="tapRecord(it.record)">
              <div class="rail-dot"><span class="dot" :style="{ background: categoryColor(it.record.category) }"></span></div>
              <div class="body">
                <span v-if="it.record.cost_cents" class="cost">{{ money(it.record.cost_cents) }}<small v-if="it.record.covered_cents">{{ money(it.record.covered_cents) }} back</small></span>
                <h4>{{ it.record.title }}</h4>
                <div class="m">{{ it.record.occurred_on }}<span v-if="it.record.ends_on"> – {{ it.record.ends_on }}</span><span v-if="it.record.contact" class="at-clinic"> · {{ it.record.contact.name }}</span></div>
                <!-- scan badge + notes + cost breakdown + custom fields, shared
                     with the record view so the two surfaces can't drift (#44). -->
                <RecordDetails :record="it.record" />
                <div v-if="it.record.attachments && it.record.attachments.length" class="atts">
                  <a v-for="a in it.record.attachments" :key="a.id" class="att" :href="api.attachmentUrl(a.id)" target="_blank" rel="noopener">
                    <img v-if="isImage(a.media_type)" :src="api.attachmentUrl(a.id)" alt="" />
                    <span v-else class="att-doc"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><path d="M14 2v6h6"/></svg></span>
                    <span class="att-name">{{ a.filename }}<small>{{ fileSize(a.size_bytes) }}</small></span>
                  </a>
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>

      <RecordSheet v-if="openRecord" :pet-id="pet.id" :pet-name="pet.name" :record="openRecord"
                   @close="openRecord = null" @saved="afterRecordChange" @deleted="afterRecordChange" />
      <ContactSheet v-if="addingContact" @close="addingContact = false" @saved="onContactAdded" @deleted="onContactDeleted" />
      <ContactSheet v-if="openContact" :contact="openContact"
                    @close="openContact = null" @saved="onContactSaved" @deleted="onContactDeleted" />
      <InsuranceSheet v-if="addingInsurance" :pet-id="pet.id" :pet-name="pet.name"
                      @close="addingInsurance = false" @saved="onInsuranceSaved" @deleted="onInsuranceDeleted" />
      <InsuranceSheet v-if="openInsurance" :pet-id="pet.id" :pet-name="pet.name" :policy="openInsurance"
                      @close="openInsurance = null" @saved="onInsuranceSaved" @deleted="onInsuranceDeleted" />
      <PhotoEditorSheet
        v-if="showPhotoEditor"
        :pet-id="pet.id" :pet-name="pet.name" :has-photo="!!photoUrl"
        @close="showPhotoEditor = false" @saved="onPhotoSaved"
      />

      <!-- Profile edit (#63): the standing facts a new-vet form asks for. -->
      <PetSheet
        v-if="editingProfile"
        :pet="pet"
        @close="editingProfile = false" @saved="onProfileSaved" @archive="startArchive"
      />
      <ArchiveSheet v-if="archiving" :pet="pet" @close="archiving = false" @saved="onArchived" />

      <!-- Ongoing care (#63): add a new entry, or view/edit an existing one. -->
      <RegimenSheet
        v-if="addingRegimen"
        :pet-id="pet.id" :pet-name="pet.name"
        @close="addingRegimen = false" @saved="onRegimenSaved"
      />
      <RegimenSheet
        v-else-if="openRegimen"
        :pet-id="pet.id" :pet-name="pet.name" :regimen="openRegimen"
        @close="openRegimen = null" @saved="onRegimenSaved" @deleted="onRegimenDeleted"
      />
    </template>
  </div>
</template>

<style scoped>
.tevent.clickable { cursor: pointer; }
.tevent.clickable:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; border-radius: 12px; }
.tevent.clickable:hover .body h4 { color: var(--brand); }
/* Milestones (#32): a warm rose marker (--milestone), distinct from category records. */
.tevent.milestone .body h4 { font-weight: 650; }
.ms-emoji { margin-right: 6px; }

/* Archived (#3): a gentle banner in the "Next up" slot, same card shape — no
   alarm colours; the wording, not the paint, carries the moment. */
.archived-banner { position: relative; z-index: 1; display: flex; gap: 12px; align-items: flex-start; margin: 14px auto 0; max-width: 360px; background: var(--surface); box-shadow: var(--shadow-sm); border-radius: var(--r); padding: 16px 18px; text-align: left; }
@media (min-width: 720px) { .archived-banner { margin: 0; flex: none; width: 280px; } }
.ab-ico { font-size: 20px; line-height: 1.3; }
.ab-txt { min-width: 0; }
.ab-txt h4 { font-size: 15px; }
.ab-date { color: var(--ink-2); font-weight: 600; }
.ab-txt p { color: var(--ink-2); font-size: 13.5px; margin: 3px 0 0; }
.archived-banner[data-kind="passed"] .ab-txt p { font-style: italic; }
.ab-acts { display: flex; gap: 14px; margin-top: 8px; }
.ab-acts button { color: var(--brand); font-weight: 650; font-size: 13px; padding: 0; }
.ab-acts button:hover { text-decoration: underline; }
.ab-txt p.ab-err { color: var(--crit); font-style: normal; }

/* Allergies (#63): a medical alert, so it leads the page and uses the critical
   tint rather than a neutral chip. */
.allergy-alert { display: flex; gap: 12px; align-items: flex-start; background: color-mix(in srgb, var(--crit) 8%, var(--surface)); border: 1px solid color-mix(in srgb, var(--crit) 25%, transparent); border-radius: 16px; padding: 12px 14px; margin-bottom: 14px; }
.allergy-alert h4 { font-size: 12.5px; color: var(--crit); font-weight: 700; text-transform: uppercase; letter-spacing: .02em; margin: 0 0 6px; }
.allergy-icon { font-size: 16px; line-height: 1.2; }

/* Ongoing care (#63): rows of what the pet is on, tap to view/edit. */
.carerow { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--border); cursor: pointer; }
.carerow:last-of-type { border-bottom: none; }
.carerow:hover .carebody b { color: var(--brand); }
.carerow:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; border-radius: 12px; }
.carerow.past { opacity: .6; }
.carebody { flex: 1 1 auto; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.carebody b { font-size: 15px; }
.carebody small { color: var(--ink-2); font-size: 12.5px; }
.carebody .reqchips { gap: 5px; margin-top: 5px; }
.carebody .reqchip { font-size: 11.5px; padding: 2px 9px; }
.care-past-h { font-size: 11.5px; color: var(--ink-3); font-weight: 700; text-transform: uppercase; letter-spacing: .02em; margin: 14px 0 2px; }

/* Care team (#20): rows of linked contacts, tap to edit, × to unlink. */
.ctrow { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--border); cursor: pointer; }
.ctrow:last-of-type { border-bottom: none; }
.ctrow:hover .ctbody b { color: var(--brand); }
.ctrow:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; border-radius: 12px; }
.ctbody { flex: 1 1 auto; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.ctbody b { font-size: 15px; }
.ctbody small { color: var(--ink-2); font-size: 12.5px; }
/* Facility requirement chips (#42) under a care-team contact. */
/* Requirement chips are global (styles.css); inside a dense care-team row they
   only need to be a touch tighter — an override, not a second definition. */
.ctbody .reqchips { gap: 5px; margin-top: 5px; }
.ctbody .reqchip { font-size: 11.5px; padding: 2px 9px; }
/* Hours (#42 follow-up): a compact label→time list under the contact. */
.cthours { display: flex; flex-wrap: wrap; gap: 4px 12px; margin-top: 4px; font-size: 12px; color: var(--ink-2); }
.cthour b { color: var(--ink); font-weight: 650; margin-right: 3px; }
.ctadd { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; margin-top: 10px; }
.ctlink { flex: 1 1 200px; }
/* the clinic name trailing a record's date on the timeline */
.at-clinic { color: var(--ink-3); }

/* Insurance (#8/#29): rows of policies/plans, tap to view. */
.insrow { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--border); cursor: pointer; }
.insrow:last-of-type { border-bottom: none; }
.insrow:hover .insbody b { color: var(--brand); }
.insrow:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; border-radius: 12px; }
.insbody { flex: 1 1 auto; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.insbody b { font-size: 15px; }
.insbody small { color: var(--ink-2); font-size: 12.5px; }
.ins-badge { display: inline-grid; place-items: center; width: 34px; height: 34px; border-radius: 10px; font-weight: 800; color: #fff; background: var(--brand); flex: 0 0 auto; }
.ins-badge[data-kind="wellness"] { background: var(--brand-2, var(--brand)); }
.ins-doc { color: var(--ink-3); display: inline-flex; }
.ins-doc svg { width: 18px; height: 18px; }
</style>
