<script setup lang="ts">
import { ref, reactive, computed } from 'vue'
import { api } from '../api'
import { sexOptions } from '../format'
import { display } from '../store'
import type { Pet, Sex } from '../types'
import SheetShell from './SheetShell.vue'

const emit = defineEmits<{ close: []; created: [pet: Pet] }>()
const saving = ref(false)
const error = ref('')

const form = reactive({
  name: '', species: 'Dog', breed: '', sex: '' as Sex | '', birth_date: '', weight: '', weight_unit: 'kg',
})
// A closed set (#72), worded for the chosen species.
const sexChoices = computed(() => sexOptions(form.species, display.breeder_terms))
const SPECIES = ['Dog', 'Cat', 'Horse', 'Bird', 'Fish', 'Rabbit', 'Reptile', 'Other']

async function submit() {
  if (!form.name.trim()) { error.value = 'Give your pet a name.'; return }
  saving.value = true
  error.value = ''
  try {
    const payload = {
      name: form.name.trim(),
      species: form.species,
      breed: form.breed || null,
      sex: form.sex || null,
      birth_date: form.birth_date || null,
      weight: form.weight ? parseFloat(form.weight) : null,
      weight_unit: form.weight ? form.weight_unit : null,
    }
    const pet = await api.createPet(payload)
    emit('created', pet)
  } catch (e) {
    error.value = (e as Error).message || 'Could not save.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <SheetShell testid="add-pet-sheet" @close="emit('close')">
    <div class="sheet-head">
      <h3>Add a pet</h3>
      <small>A new member of the household.</small>
    </div>

    <div class="sheet-body">
      <div class="field">
        <label>Name</label>
        <input v-model="form.name" placeholder="Biscuit" data-testid="add-pet-name" autofocus />
      </div>
      <div class="field-row">
        <div class="field">
          <label>Species</label>
          <select v-model="form.species" data-testid="add-pet-species">
            <option v-for="s in SPECIES" :key="s" :value="s">{{ s }}</option>
          </select>
        </div>
        <div class="field">
          <label>Breed</label>
          <input v-model="form.breed" placeholder="Golden Retriever" data-testid="add-pet-breed" />
        </div>
      </div>
      <div class="field-row">
        <div class="field">
          <label>Sex</label>
          <select v-model="form.sex" data-testid="add-pet-sex">
            <option value="">Not recorded</option>
            <option v-for="o in sexChoices" :key="o.value" :value="o.value">{{ o.label }}</option>
          </select>
        </div>
        <div class="field">
          <label>Born</label>
          <input v-model="form.birth_date" type="date" data-testid="add-pet-birth" />
        </div>
      </div>
      <div class="field-row">
        <div class="field">
          <label>Weight</label>
          <input v-model="form.weight" type="number" step="0.1" placeholder="32.4" data-testid="add-pet-weight" />
        </div>
        <div class="field" style="flex:0 0 90px">
          <label>Unit</label>
          <select v-model="form.weight_unit" data-testid="add-pet-unit">
            <option>kg</option><option>lb</option><option>g</option>
          </select>
        </div>
      </div>
      <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>
    </div>

    <div class="sheet-foot">
      <button class="btn sec" data-testid="add-pet-cancel" @click="emit('close')">Cancel</button>
      <button class="btn primary" data-testid="add-pet-save" :disabled="saving" @click="submit">
        {{ saving ? 'Saving…' : 'Add pet' }}
      </button>
    </div>
  </SheetShell>
</template>
