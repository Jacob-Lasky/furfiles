<script setup lang="ts">
import { ref } from 'vue'
import { api } from '../api'
import type { ReminderWithPet } from '../types'
import SheetShell from './SheetShell.vue'

// Compact detail for a calendar reminder (#16). Reminders aren't records, so
// they get their own small sheet: mark done / not done, delete, or jump to the
// pet. The record edit sheet stays reserved for records.
const props = defineProps<{ reminder: ReminderWithPet }>()
const emit = defineEmits<{ close: []; changed: []; deleted: [] }>()
const busy = ref(false)
const error = ref('')

async function toggleDone() {
  busy.value = true
  error.value = ''
  try {
    await api.updateReminder(props.reminder.id, { done: !props.reminder.done })
    emit('changed')
  } catch (e) { error.value = e.message || 'Could not update.'; busy.value = false }
}

async function remove() {
  if (!window.confirm(`Delete reminder “${props.reminder.title}”?`)) return
  busy.value = true
  error.value = ''
  try {
    await api.deleteReminder(props.reminder.id)
    emit('deleted')
  } catch (e) { error.value = e.message || 'Could not delete.'; busy.value = false }
}
</script>

<template>
  <SheetShell testid="reminder-sheet" @close="emit('close')">
    <div class="sheet-head">
      <h3>Reminder</h3>
      <small>{{ reminder.pet_name }} · {{ reminder.due_on }}</small>
    </div>
    <div class="sheet-body">
      <div class="rs-title" :class="{ done: reminder.done }">{{ reminder.title }}</div>
      <div class="rs-meta">
        <router-link :to="`/pets/${reminder.pet_id}`" class="rs-link" @click="emit('close')">Open {{ reminder.pet_name }}’s profile →</router-link>
      </div>
      <p v-if="error" style="color:var(--crit);font-size:13px;margin:10px 0 0">{{ error }}</p>
    </div>
    <div class="sheet-foot">
      <button class="btn danger" type="button" :disabled="busy" @click="remove">Delete</button>
      <button class="btn primary" type="button" :disabled="busy" @click="toggleDone">
        {{ reminder.done ? 'Mark not done' : 'Mark done' }}
      </button>
    </div>
  </SheetShell>
</template>

<style scoped>
.rs-title { font-size: 18px; font-weight: 700; font-family: var(--font-d); }
.rs-title.done { text-decoration: line-through; color: var(--ink-3); }
.rs-meta { margin-top: 12px; }
.rs-link { color: var(--brand); font-weight: 650; font-size: 14px; }
</style>
