<script setup lang="ts">
// Edit a pet's PROFILE — the standing facts that are true of the animal itself
// (#63). Until this existed a pet's microchip, birth date and gotcha day could
// only be set at creation and were then frozen forever, because `updatePet` was
// only ever called by InsuranceSheet to sync the insurer name.
//
// Unlike ContactSheet / InsuranceSheet / RecordSheet this is edit-only, with no
// read-only "view" half: the pet's PROFILE PAGE is already that view, so a
// second one inside a sheet would be the same content twice.
//
// Every field except name/species is sent as `null` when blanked, so the editor
// can CLEAR a value — a mistyped microchip number must not be permanent.
import { ref, reactive, computed } from 'vue'
import { api } from '../api'
import { labeledToRows, rowsToLabeled, sexOptions } from '../format'
import type { LabeledRow } from '../format'
import { display } from '../store'
import type { Pet, Sex, UpdatePetInput } from '../types'
import SheetShell from './SheetShell.vue'

const props = defineProps<{ pet: Pet }>()
// `archive` hands off to the ArchiveSheet (#3): leaving the household is a
// different, gentler decision than editing a field, so it gets its own sheet.
const emit = defineEmits<{ close: []; saved: [pet: Pet]; archive: [] }>()

const saving = ref(false)
const error = ref('')

const SPECIES = ['Dog', 'Cat', 'Horse', 'Bird', 'Fish', 'Rabbit', 'Reptile', 'Other']
// A species typed by hand (or seeded) that isn't in the list must not be
// silently rewritten to "Dog" by the select, so widen the options to include it.
const speciesOptions = computed(() =>
  SPECIES.some((s) => s.toLowerCase() === props.pet.species.toLowerCase())
    ? SPECIES
    : [props.pet.species, ...SPECIES],
)

const form = reactive({
  name: props.pet.name,
  species: props.pet.species,
  breed: props.pet.breed || '',
  color: props.pet.color || '',
  sex: (props.pet.sex || '') as Sex | '',
  birth_date: props.pet.birth_date || '',
  gotcha_date: props.pet.gotcha_date || '',
  microchip: props.pet.microchip || '',
})

// Sex is a closed set (#72), worded for the species picked above, so a horse
// reads Stallion / Mare / Gelding as it will on the profile.
const sexChoices = computed(() => sexOptions(form.species, display.breeder_terms))

// Allergies reuse the LabeledDetail row editor (contacts' requirements/hours,
// a policy's coverages/exclusions): label = allergen, detail = reaction.
const allergies = ref<LabeledRow[]>(labeledToRows(props.pet.allergies))
function addAllergy() { allergies.value.push({ label: '', detail: '' }) }
function removeAllergy(i: number) { allergies.value.splice(i, 1) }

async function submit() {
  if (!form.name.trim()) { error.value = 'Give your pet a name.'; return }
  if (!form.species.trim()) { error.value = 'Pick a species.'; return }
  saving.value = true
  error.value = ''
  try {
    const payload: UpdatePetInput = {
      name: form.name.trim(),
      species: form.species.trim(),
      breed: form.breed.trim() || null,
      color: form.color.trim() || null,
      sex: form.sex || null,
      birth_date: form.birth_date || null,
      gotcha_date: form.gotcha_date || null,
      microchip: form.microchip.trim() || null,
      allergies: rowsToLabeled(allergies.value),
    }
    emit('saved', await api.updatePet(props.pet.id, payload))
  } catch (e) {
    error.value = (e as Error).message || 'Could not save.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <SheetShell testid="pet-sheet" @close="emit('close')">
    <div class="sheet-head">
      <h3>Edit {{ pet.name }}'s profile</h3>
      <small>The standing facts — the ones every new-vet form asks for.</small>
    </div>

    <div class="sheet-body">
      <div class="field-row">
        <div class="field">
          <label>Name</label>
          <input v-model="form.name" data-testid="pet-name-input" autofocus />
        </div>
        <div class="field" style="flex:0 0 140px">
          <label>Species</label>
          <select v-model="form.species" data-testid="pet-species-select">
            <option v-for="s in speciesOptions" :key="s" :value="s">{{ s }}</option>
          </select>
        </div>
      </div>

      <div class="field-row">
        <div class="field">
          <label>Breed <span class="lbl-opt">optional</span></label>
          <input v-model="form.breed" placeholder="Mix, Australian Shepherd" data-testid="pet-breed-input" />
        </div>
        <div class="field">
          <label>Colour <span class="lbl-opt">optional</span></label>
          <input v-model="form.color" placeholder="Blue Merle" data-testid="pet-color-input" />
        </div>
      </div>

      <div class="field">
        <label>Sex <span class="lbl-opt">optional</span></label>
        <select v-model="form.sex" data-testid="pet-sex-input">
          <option value="">Not recorded</option>
          <option v-for="o in sexChoices" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </div>

      <div class="field-row">
        <div class="field">
          <label>Birthday <span class="lbl-opt">optional</span></label>
          <input v-model="form.birth_date" type="date" data-testid="pet-birth-input" />
        </div>
        <div class="field">
          <label>Gotcha day <span class="lbl-opt">optional</span></label>
          <input v-model="form.gotcha_date" type="date" data-testid="pet-gotcha-input" />
        </div>
      </div>

      <div class="field">
        <label>Microchip # <span class="lbl-opt">optional</span></label>
        <input v-model="form.microchip" class="mono" inputmode="numeric" placeholder="000000000000202" data-testid="pet-microchip-input" />
      </div>

      <!-- Allergies: allergen → reaction. Leaving this empty IS the intake
           form's "No known allergies". -->
      <div class="field">
        <label>Known allergies <span class="lbl-opt">allergen and what it does</span></label>
        <div v-for="(a, i) in allergies" :key="i" class="kvrow" data-testid="allergy-row">
          <input v-model="a.label" class="kv-k" placeholder="e.g. Chicken" aria-label="Allergen" />
          <input v-model="a.detail" class="kv-v" placeholder="e.g. hives, ear infections" aria-label="Reaction" />
          <button class="fx" type="button" aria-label="Remove allergy" @click="removeAllergy(i)">×</button>
        </div>
        <button class="addrow" type="button" data-testid="add-allergy" @click="addAllergy">+ Add an allergy</button>
      </div>

      <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>

      <!-- Leaving the household (#3). Quiet and last: rare, and never a mis-tap
           away from Save. Archived pets manage this from their profile banner. -->
      <div v-if="!pet.archived_at" class="leave">
        <button type="button" class="leave-btn" data-testid="pet-archive-open" @click="emit('archive')">
          Has {{ pet.name }} passed away or moved on?
        </button>
        <small>Every record is kept, and you can bring them back anytime.</small>
      </div>
    </div>

    <div class="sheet-foot">
      <button class="btn sec" type="button" data-testid="pet-cancel" :disabled="saving" @click="emit('close')">Cancel</button>
      <button class="btn primary" type="button" data-testid="pet-save" :disabled="saving" @click="submit">
        {{ saving ? 'Saving…' : 'Save changes' }}
      </button>
    </div>
  </SheetShell>
</template>

<style scoped>
.leave { margin-top: 20px; padding-top: 14px; border-top: 1px solid var(--border); }
.leave-btn { color: var(--ink-2); font-weight: 600; font-size: 13.5px; text-decoration: underline; text-underline-offset: 3px; padding: 0; }
.leave-btn:hover { color: var(--brand); }
.leave small { display: block; color: var(--ink-3); font-size: 12px; margin-top: 3px; }
</style>
