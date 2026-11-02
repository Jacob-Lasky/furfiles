<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '../api'
import { money, categoryColor, todayISO, CATEGORIES, monthMilestones, upcomingMilestones, milestoneIcon } from '../format'
import type { PetSummary, MedicalRecord, ReminderWithPet, RecordLike } from '../types'
import RecordSheet from '../components/RecordSheet.vue'
import ReminderSheet from '../components/ReminderSheet.vue'

const router = useRouter()

// Every dated thing across the household, on a calendar (#6): past records and
// upcoming reminders, filterable by pet and by type, in a month grid or a
// chronological agenda. Events are normalized to one shape so both views — and
// the tap-to-open detail sheets (#16) — read the same fields.
interface CalEvent {
  key: string
  kind: 'record' | 'reminder' | 'milestone'
  date: string
  petId: string
  petName: string
  title: string
  category?: string
  cost_cents?: number | null
  done?: boolean
  subkind?: string
  raw?: MedicalRecord | ReminderWithPet // the source row, for the detail sheets
}
// One month-grid cell: either a leading blank or a real day carrying its events.
interface Cell { key: string; blank: boolean; day: number | null; events: CalEvent[]; isToday: boolean }

const events = ref<CalEvent[]>([])
const pets = ref<PetSummary[]>([])
const loading = ref(true)
const error = ref('')
const cursor = ref(new Date())
const view = ref<'month' | 'agenda'>('month')
const petFilter = ref('') // '' = all pets, else a pet id
const typeFilter = ref('') // '' = all, a category, or 'reminder'
// Which detail sheet is open (#16), if any.
const openRecord = ref<{ petId: string; petName: string; record: RecordLike } | null>(null)
const openReminder = ref<ReminderWithPet | null>(null)

const monthLabel = computed(() =>
  cursor.value.toLocaleString(undefined, { month: 'long', year: 'numeric' }),
)

// Parse an ISO date to its calendar parts WITHOUT going through the local/UTC
// Date parsing that shifts a bare 'YYYY-MM-DD' across timezones.
function ymd(iso: string) {
  const [y, m, d] = String(iso).split('-').map(Number)
  return { y, m: m - 1, d }
}

// Shared by every view. Type filter maps to a kind ('reminder'/'milestone') or,
// for a specific category, a record in that category.
function passesFilter(e: CalEvent) {
  if (petFilter.value && e.petId !== petFilter.value) return false
  if (typeFilter.value === '') return true
  if (typeFilter.value === 'reminder') return e.kind === 'reminder'
  if (typeFilter.value === 'milestone') return e.kind === 'milestone'
  return e.kind === 'record' && e.category === typeFilter.value
}

const filtered = computed(() => events.value.filter(passesFilter))

// Recurring milestone anniversaries as normalized events for the agenda: the
// NEXT occurrence per active pet (#32). Past anniversaries would recur forever,
// so the agenda shows only what's ahead.
const milestoneAgenda = computed<CalEvent[]>(() =>
  upcomingMilestones(pets.value)
    .map((m): CalEvent => ({ key: `ms-${m.petId}-${m.subkind}`, kind: 'milestone', ...m }))
    .filter(passesFilter),
)

const grid = computed<Cell[]>(() => {
  const y = cursor.value.getFullYear()
  const m = cursor.value.getMonth()
  const first = new Date(y, m, 1).getDay()
  const days = new Date(y, m + 1, 0).getDate()
  const today = new Date()
  const byDay: Record<number, CalEvent[]> = {}
  for (const e of filtered.value) {
    const p = ymd(e.date)
    if (p.y === y && p.m === m) (byDay[p.d] ||= []).push(e)
  }
  // Recurring milestone anniversaries in this month (#32), active pets only.
  for (const ms of monthMilestones(pets.value, y, m)) {
    const e: CalEvent = {
      key: `ms-${ms.petId}-${ms.subkind}-${ms.day}`, kind: 'milestone', subkind: ms.subkind,
      petId: ms.petId, petName: ms.petName, title: ms.title,
      date: `${y}-${String(m + 1).padStart(2, '0')}-${String(ms.day).padStart(2, '0')}`,
    }
    if (passesFilter(e)) (byDay[ms.day] ||= []).push(e)
  }
  const cells: Cell[] = []
  for (let i = 0; i < first; i++) cells.push({ key: `b${i}`, blank: true, day: null, events: [], isToday: false })
  for (let d = 1; d <= days; d++) {
    cells.push({
      key: `d${d}`, blank: false, day: d, events: byDay[d] || [],
      isToday: today.getFullYear() === y && today.getMonth() === m && today.getDate() === d,
    })
  }
  return cells
})

// Agenda: everything (not just the visible month), split into what's still
// ahead (soonest first) and what's already happened (most recent first). Empty
// sections drop out so the template renders one row block per non-empty section.
const agendaSections = computed(() => {
  const today = todayISO()
  const up = [...filtered.value.filter((e) => e.date >= today), ...milestoneAgenda.value]
    .sort((a, b) => a.date.localeCompare(b.date))
  const past = filtered.value.filter((e) => e.date < today).sort((a, b) => b.date.localeCompare(a.date))
  return [{ label: 'Upcoming', items: up }, { label: 'Earlier', items: past }].filter((s) => s.items.length)
})

function shift(n: number) { cursor.value = new Date(cursor.value.getFullYear(), cursor.value.getMonth() + n, 1) }

async function load() {
  try {
    const petList = await api.pets(true)
    pets.value = petList
    const [recordsByPet, reminders] = await Promise.all([
      Promise.all(petList.map((p) =>
        api.records(p.id).then((rs) => rs.map((r): CalEvent => ({
          key: `r${r.id}`, kind: 'record', date: r.occurred_on, petId: p.id, petName: p.name,
          title: r.title, category: r.category, cost_cents: r.cost_cents, raw: r,
        }))),
      )),
      api.allReminders(),
    ])
    const rem = (reminders || []).map((r): CalEvent => ({
      key: `m${r.id}`, kind: 'reminder', date: r.due_on, petId: r.pet_id, petName: r.pet_name,
      title: r.title, done: r.done, raw: r,
    }))
    events.value = [...recordsByPet.flat(), ...rem]
  } catch (e) { error.value = e.message } finally { loading.value = false }
}

// Tap an event (#16): a record opens the shared edit sheet, a reminder its own
// compact sheet. Both reload the calendar on save/delete so it stays honest.
function openEvent(e: CalEvent) {
  if (e.kind === 'record') openRecord.value = { petId: e.petId, petName: e.petName, record: e.raw as RecordLike }
  else if (e.kind === 'reminder') openReminder.value = e.raw as ReminderWithPet
  else if (e.kind === 'milestone') router.push(`/pets/${e.petId}`) // derived, not editable → jump to the pet
}
function afterRecordChange() { openRecord.value = null; load() }
function afterReminderChange() { openReminder.value = null; load() }

onMounted(load)
</script>

<template>
  <div>
    <div class="apphead">
      <div>
        <div class="eyebrow">Appointments &amp; records</div>
        <h1>{{ view === 'month' ? monthLabel : 'Agenda' }}</h1>
        <div class="sub">Every visit, and what it cost.</div>
      </div>
      <div class="head-actions">
        <div class="segmented" role="tablist" aria-label="Calendar view">
          <button :class="{ on: view === 'month' }" role="tab" :aria-selected="view === 'month'" @click="view = 'month'">Month</button>
          <button :class="{ on: view === 'agenda' }" role="tab" :aria-selected="view === 'agenda'" @click="view = 'agenda'">Agenda</button>
        </div>
        <template v-if="view === 'month'">
          <button class="iconbtn" @click="shift(-1)" aria-label="Previous month"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 18l-6-6 6-6"/></svg></button>
          <button class="iconbtn" @click="shift(1)" aria-label="Next month"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18l6-6-6-6"/></svg></button>
        </template>
      </div>
    </div>

    <div v-if="loading" class="loading">Loading calendar…</div>
    <div v-else-if="error" class="errbox">{{ error }}</div>
    <div v-else class="pad">
      <!-- Filters: by pet, by type. -->
      <div class="calfilters">
        <select v-model="petFilter" aria-label="Filter by pet">
          <option value="">All pets</option>
          <option v-for="p in pets" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <select v-model="typeFilter" aria-label="Filter by type">
          <option value="">All types</option>
          <option v-for="c in CATEGORIES" :key="c" :value="c">{{ c }}</option>
          <option value="reminder">Reminders</option>
          <option value="milestone">Milestones</option>
        </select>
      </div>

      <!-- Month grid -->
      <div v-if="view === 'month'" class="card" style="overflow:hidden">
        <div class="cwk"><span>S</span><span>M</span><span>T</span><span>W</span><span>T</span><span>F</span><span>S</span></div>
        <div class="cgrid">
          <div v-for="c in grid" :key="c.key" class="cday" :class="{ blank: c.blank, today: c.isToday }" data-testid="calendar-day">
            <span v-if="!c.blank" class="dn">{{ c.day }}</span>
            <button v-for="e in c.events" :key="e.key" type="button" class="cev" data-testid="calendar-event" :data-kind="e.kind" :class="{ reminder: e.kind === 'reminder', milestone: e.kind === 'milestone', done: e.done }" :style="{ '--ec': e.kind === 'reminder' ? 'var(--brand)' : e.kind === 'milestone' ? 'var(--milestone)' : categoryColor(e.category) }" @click="openEvent(e)">
              <template v-if="e.kind === 'milestone'"><span aria-hidden="true">{{ milestoneIcon(e.subkind) }}</span> {{ e.title }}</template>
              <template v-else>{{ e.petName }} · {{ e.title }}<b v-if="e.cost_cents">{{ money(e.cost_cents) }}</b></template>
            </button>
          </div>
        </div>
      </div>

      <!-- Agenda: one row block per non-empty section (Upcoming / Earlier). -->
      <div v-else class="agenda">
        <template v-for="sec in agendaSections" :key="sec.label">
          <div class="section-h"><h3>{{ sec.label }}</h3><span>{{ sec.items.length }}</span></div>
          <div class="card card-pad">
            <button v-for="e in sec.items" :key="e.key" type="button" class="arow" data-testid="calendar-event" :data-kind="e.kind" :class="{ reminder: e.kind === 'reminder', milestone: e.kind === 'milestone', done: e.done }" @click="openEvent(e)">
              <span class="adot" :style="{ background: e.kind === 'reminder' ? 'var(--brand)' : e.kind === 'milestone' ? 'var(--milestone)' : categoryColor(e.category) }"></span>
              <span class="abody">
                <b v-if="e.kind === 'milestone'"><span aria-hidden="true">{{ milestoneIcon(e.subkind) }}</span> {{ e.title }}</b>
                <b v-else>{{ e.petName }} · {{ e.title }}</b>
                <small>{{ e.date }}<span v-if="e.kind === 'record' && e.category"> · {{ e.category }}</span></small>
              </span>
              <span v-if="e.cost_cents" class="mono acost">{{ money(e.cost_cents) }}</span>
            </button>
          </div>
        </template>
        <div v-if="!agendaSections.length" class="card card-pad" style="color:var(--ink-3);font-size:14px">Nothing to show yet.</div>
      </div>
    </div>

    <RecordSheet v-if="openRecord" :pet-id="openRecord.petId" :pet-name="openRecord.petName" :record="openRecord.record"
                 @close="openRecord = null" @saved="afterRecordChange" @deleted="afterRecordChange" />
    <ReminderSheet v-if="openReminder" :reminder="openReminder"
                   @close="openReminder = null" @changed="afterReminderChange" @deleted="afterReminderChange" />
  </div>
</template>

<style scoped>
.segmented { display: inline-flex; background: var(--surface-2); border-radius: 11px; padding: 3px; gap: 2px; }
.segmented button { font-size: 13px; font-weight: 650; padding: 6px 13px; border-radius: 8px; color: var(--ink-2); }
.segmented button.on { background: var(--surface); color: var(--ink); box-shadow: var(--shadow-sm, 0 1px 2px rgba(0,0,0,.12)); }
.calfilters { display: flex; gap: 10px; margin-bottom: 14px; }
.calfilters select { flex: 1; padding: 10px 12px; border-radius: 12px; border: 1px solid var(--border-2); background: var(--surface); color: var(--ink); font-size: 13.5px; }
.calfilters select:focus { outline: 2px solid var(--brand); outline-offset: 1px; }
.cwk { display: grid; grid-template-columns: repeat(7, 1fr); padding: 12px 6px 4px; }
.cwk span { text-align: center; font-size: 11px; font-weight: 700; color: var(--ink-3); }
.cgrid { display: grid; grid-template-columns: repeat(7, 1fr); }
.cday { min-height: 70px; padding: 5px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 3px; }
.cday.blank { background: var(--surface-2); }
.dn { font-size: 11px; color: var(--ink-3); font-family: var(--font-m); }
.cday.today .dn { background: var(--brand); color: #fff; width: 19px; height: 19px; border-radius: 50%; display: grid; place-items: center; font-weight: 700; }
.cev { font-size: 9.5px; font-weight: 700; line-height: 1.2; padding: 3px 5px; border-radius: 6px; background: color-mix(in srgb, var(--ec) 16%, transparent); color: var(--ec); border-left: 2px solid var(--ec); text-align: left; }
.cev b { display: block; font-family: var(--font-m); opacity: .85; }
.cev.reminder { background: transparent; border: 1px dashed var(--ec); border-left-width: 2px; }
/* Milestones (#32) are warm rose (--milestone), filled — distinct from category
   records and dashed reminders; the color is set inline via --ec / --milestone. */
.cev.done { opacity: .45; text-decoration: line-through; }
.arow { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--border); text-align: left; width: 100%; }
.arow:last-child { border-bottom: none; }
.arow.done { opacity: .5; }
.arow.done .abody b { text-decoration: line-through; }
.adot { flex: none; width: 10px; height: 10px; border-radius: 50%; }
.arow.reminder .adot { background: transparent !important; border: 2px dashed var(--brand); }
.abody { flex: 1; display: flex; flex-direction: column; min-width: 0; }
.abody b { font-size: 14px; font-weight: 650; }
.abody small { font-size: 12px; color: var(--ink-3); font-family: var(--font-m); }
.acost { font-weight: 700; font-size: 13px; }
@media (min-width: 720px) { .cday { min-height: 100px; } .cev { font-size: 11px; } }
</style>
