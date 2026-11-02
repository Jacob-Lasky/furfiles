<script setup lang="ts">
// "Update to Spayed female?" (#72): the pet has a record that looks like a
// spay or neuter while its sex still says intact. The server derives the
// suggestion on every read (`PetDetail.sex_suggestion`) and stores nothing, so
// accepting it is an ordinary profile PATCH and the chip simply stops coming
// back. The record it came from is one tap away, so the evidence can be checked
// (or the record corrected) before the profile changes.
import { ref, computed } from 'vue'
import { api } from '../api'
import { sexLabel, longDate } from '../format'
import { display } from '../store'
import type { PetDetail } from '../types'

const props = defineProps<{ pet: PetDetail }>()
const emit = defineEmits<{ updated: []; openRecord: [recordId: number] }>()

const saving = ref(false)
const error = ref('')
const sug = computed(() => props.pet.sex_suggestion)
const label = computed(() => (sug.value ? sexLabel(sug.value.sex, props.pet.species, display.breeder_terms) : ''))

async function accept() {
  if (!sug.value) return
  saving.value = true
  error.value = ''
  try {
    await api.updatePet(props.pet.id, { sex: sug.value.sex })
    emit('updated')
  } catch (e) {
    error.value = (e as Error).message || 'Could not update.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div v-if="sug" class="sexsug" data-testid="sex-suggestion">
    <button type="button" class="sexsug-go" data-testid="sex-suggestion-accept" :disabled="saving" @click="accept">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true"><path d="M5 12l5 5 9-10"/></svg>
      Update to {{ label }}?
    </button>
    <button type="button" class="sexsug-rec" data-testid="sex-suggestion-record" @click="emit('openRecord', sug.record_id)">
      {{ sug.record_title }} · {{ longDate(sug.occurred_on) }}
    </button>
    <p v-if="error" class="sexsug-err">{{ error }}</p>
  </div>
</template>

<style scoped>
.sexsug { display: inline-flex; align-items: center; flex-wrap: wrap; justify-content: center; gap: 4px; margin-top: 10px; padding: 3px; border-radius: 999px; background: color-mix(in srgb, var(--brand) 10%, var(--surface)); box-shadow: var(--shadow-sm); }
.sexsug-go { display: inline-flex; align-items: center; gap: 5px; padding: 5px 12px; border-radius: 999px; background: var(--brand); color: #fff; font: inherit; font-size: 12.5px; font-weight: 700; cursor: pointer; }
.sexsug-go:disabled { opacity: .6; cursor: default; }
.sexsug-go svg { width: 13px; height: 13px; }
.sexsug-rec { padding: 5px 10px 5px 4px; color: var(--ink-2); font: inherit; font-size: 12px; font-weight: 600; text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }
.sexsug-rec:hover { color: var(--brand); }
.sexsug-err { flex-basis: 100%; margin: 2px 10px 4px; color: var(--crit); font-size: 12px; }
</style>
