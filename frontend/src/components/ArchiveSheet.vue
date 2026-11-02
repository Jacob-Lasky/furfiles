<script setup lang="ts">
// Archive a pet, or edit an existing archive (#3, #37). Two honest kinds: a pet
// that PASSED is remembered (memorial wording, date of passing), and a pet that
// was REHOMED — sold, adopted out, gone to live with family — is alive, so it
// gets no memorial wording at all. Nothing is deleted either way: every record
// stays, reminders simply stop prompting, and "Bring back" on the profile undoes
// it. Re-posting the archive endpoint is the edit, so this sheet serves both.
import { ref, computed } from 'vue'
import { api } from '../api'
import { todayISO } from '../format'
import type { ArchiveKind, Pet } from '../types'
import SheetShell from './SheetShell.vue'

const props = defineProps<{ pet: Pet }>()
const emit = defineEmits<{ close: []; saved: [pet: Pet] }>()

const editing = computed(() => !!props.pet.archived_at)
const kind = ref<ArchiveKind>(props.pet.archive_kind === 'rehomed' ? 'rehomed' : 'passed')
const date = ref(props.pet.archived_at ? props.pet.archived_at.slice(0, 10) : todayISO())
const note = ref(props.pet.memorial_note || '')
const saving = ref(false)
const error = ref('')

const copy = computed(() =>
  kind.value === 'passed'
    ? { date: 'Date of passing', note: 'A few words to remember them by', placeholder: 'Fifteen very good years.' }
    : { date: 'Date they moved on', note: 'Where they went', placeholder: 'Retired to a trainer in Riverbend' },
)

async function submit() {
  saving.value = true
  error.value = ''
  try {
    emit('saved', await api.archivePet(props.pet.id, {
      kind: kind.value,
      archived_at: date.value || null,
      memorial_note: note.value.trim() || null,
    }))
  } catch (e) {
    error.value = (e as Error).message || 'Could not save.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <SheetShell testid="archive-sheet" @close="emit('close')">
    <div class="sheet-head">
      <h3>{{ editing ? `Update ${pet.name}'s page` : `Is ${pet.name} no longer with you?` }}</h3>
      <small>Nothing is deleted. Every record stays, and reminders stop prompting.</small>
    </div>

    <div class="sheet-body">
      <div class="akinds" role="radiogroup" aria-label="What happened">
        <label class="akind" :class="{ on: kind === 'passed' }" data-testid="archive-kind-passed">
          <input v-model="kind" type="radio" name="archive-kind" value="passed" />
          <span class="akind-ico" aria-hidden="true">🕊️</span>
          <span><b>{{ pet.name }} passed away</b><small>Remembered on Home, with every record kept.</small></span>
        </label>
        <label class="akind" :class="{ on: kind === 'rehomed' }" data-testid="archive-kind-rehomed">
          <input v-model="kind" type="radio" name="archive-kind" value="rehomed" />
          <span class="akind-ico" aria-hidden="true">🌿</span>
          <span><b>{{ pet.name }} moved on</b><small>Sold, rehomed or living with family. Listed under Past companions.</small></span>
        </label>
      </div>

      <div class="field">
        <label>{{ copy.date }}</label>
        <input v-model="date" type="date" data-testid="archive-date" />
      </div>
      <div class="field">
        <label>{{ copy.note }} <span class="lbl-opt">optional</span></label>
        <textarea v-model="note" rows="2" :placeholder="copy.placeholder" data-testid="archive-note"></textarea>
      </div>

      <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>
    </div>

    <div class="sheet-foot">
      <button class="btn sec" type="button" data-testid="archive-cancel" :disabled="saving" @click="emit('close')">Cancel</button>
      <button class="btn primary" type="button" data-testid="archive-save" :disabled="saving" @click="submit">
        {{ saving ? 'Saving…' : editing ? 'Save changes' : kind === 'passed' ? `Remember ${pet.name}` : `Move to past companions` }}
      </button>
    </div>
  </SheetShell>
</template>

<style scoped>
.akinds { display: flex; flex-direction: column; gap: 10px; margin-bottom: 16px; }
.akind { display: flex; align-items: flex-start; gap: 12px; padding: 13px 14px; border: 1px solid var(--border-2); border-radius: 16px; background: var(--surface); cursor: pointer; }
.akind.on { border-color: var(--brand); background: var(--brand-soft); }
.akind input { position: absolute; opacity: 0; pointer-events: none; }
.akind:focus-within { outline: 2px solid var(--brand); outline-offset: 2px; }
.akind-ico { font-size: 20px; line-height: 1.2; }
.akind b { display: block; font-size: 15px; }
.akind small { display: block; color: var(--ink-2); font-size: 12.5px; margin-top: 2px; }
</style>
