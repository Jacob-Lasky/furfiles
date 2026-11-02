<script setup lang="ts">
import { ref, reactive, watch, onMounted, onBeforeUnmount } from 'vue'
import { api } from '../api'
import { display, setDisplay } from '../store'
import type { IngestionInput, ProviderModel } from '../types'

// Document-scanning settings (#4): bring-your-own-key, off by default. The key
// is write-only — the server reports only whether one is stored (`has_key`).
const loading = ref(true)
const saving = ref(false)
const error = ref('')
const saved = ref(false)
const hasKey = ref(false)

const form = reactive({ model: '', enabled: false, api_key: '' })

// The model picker (#67): the provider's own list for the stored key, or for a
// key pasted here before saving. "Custom…" (or no list at all) falls back to
// the free-text input, so a model the list doesn't show can still be typed.
const CUSTOM = '__custom__'
const models = ref<ProviderModel[]>([])
const choice = ref(CUSTOM)
const modelsLoading = ref(false)
const modelsError = ref('') // the stored key's list failed
const keyError = ref('') // the pasted key's list failed
// The pasted key itself was refused (another vendor's key, or Anthropic said
// no): a 4xx, unlike a 5xx outage. Save stays off so a known-bad key can't
// replace a working one.
const keyRefused = ref(false)
// Only the newest request may update the picker: a slow answer for an earlier
// keystroke must not overwrite the list for the key now in the box.
let modelsSeq = 0
let keyTimer: ReturnType<typeof setTimeout> | undefined
/** Wait this long after the last keystroke before asking about a pasted key. */
const KEY_DEBOUNCE_MS = 500
/** Shorter than any real key; stops a half-typed prefix being sent. */
const MIN_KEY_CHARS = 10

// A saved model that isn't in the list shows as Custom with its value, so the
// picker never silently swaps the stored model for the first option.
function syncChoice() {
  choice.value = models.value.some((m) => m.id === form.model) ? form.model : CUSTOM
}
watch(choice, (c) => {
  if (c !== CUSTOM) form.model = c
})

async function loadModels(apiKey?: string) {
  const seq = ++modelsSeq
  modelsLoading.value = true
  try {
    const list = await api.ingestionModels(apiKey)
    if (seq !== modelsSeq) return
    models.value = list
    modelsError.value = ''
    keyError.value = ''
    syncChoice()
  } catch (e) {
    if (seq !== modelsSeq) return
    models.value = []
    choice.value = CUSTOM
    if (apiKey === undefined) {
      modelsError.value = e.message
    } else {
      keyError.value = e.message
      keyRefused.value = e.status >= 400 && e.status < 500
    }
  } finally {
    if (seq === modelsSeq) modelsLoading.value = false
  }
}

function clearModels() {
  models.value = []
  modelsError.value = ''
  keyError.value = ''
  keyRefused.value = false
  choice.value = CUSTOM
}

// Back to the stored key's list (or to nothing, with no stored key).
function showStoredModels() {
  clearModels()
  if (hasKey.value) return loadModels()
  modelsLoading.value = false
}

// Every edit invalidates whatever is in flight and clears the list, because
// both belong to a key that is no longer in the box. The model value itself is
// untouched (Custom shows it until the new list arrives and re-selects it).
watch(() => form.api_key, (raw) => {
  clearTimeout(keyTimer)
  modelsSeq++
  const key = raw.trim()
  if (!key) return void showStoredModels()
  clearModels()
  modelsLoading.value = key.length >= MIN_KEY_CHARS
  if (modelsLoading.value) keyTimer = setTimeout(() => loadModels(key), KEY_DEBOUNCE_MS)
})
onBeforeUnmount(() => clearTimeout(keyTimer))

async function load() {
  loading.value = true
  try {
    const s = await api.ingestion()
    form.model = s.model
    form.enabled = s.enabled
    hasKey.value = s.has_key
  } catch (e) {
    error.value = e.message
  } finally {
    loading.value = false
  }
  if (hasKey.value) loadModels()
}
onMounted(load)

async function save() {
  saving.value = true
  error.value = ''
  saved.value = false
  try {
    const payload: IngestionInput = { model: form.model, enabled: form.enabled }
    if (form.api_key.trim()) payload.api_key = form.api_key.trim()
    const s = await api.saveIngestion(payload)
    hasKey.value = s.has_key
    form.enabled = s.enabled
    form.model = s.model
    form.api_key = '' // the watcher reloads the (now stored) key's list
    syncChoice()
    saved.value = true
  } catch (e) {
    error.value = e.message
  } finally {
    saving.value = false
  }
}

// Display (#72): wording only, saved as soon as it's toggled. The shared store
// is updated from the server's answer, so every profile re-words at once.
const displaySaving = ref(false)
const displayError = ref('')
async function setBreederTerms(on: boolean) {
  displaySaving.value = true
  displayError.value = ''
  try {
    setDisplay(await api.saveDisplaySettings({ breeder_terms: on }))
  } catch (e) {
    displayError.value = e.message
  } finally {
    displaySaving.value = false
  }
}

async function disconnect() {
  saving.value = true
  error.value = ''
  saved.value = false
  try {
    const s = await api.clearIngestion()
    hasKey.value = s.has_key
    form.enabled = s.enabled
    form.api_key = ''
    showStoredModels()
  } catch (e) {
    error.value = e.message
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="pad">
    <div class="section-h"><h3>Settings</h3></div>

    <div v-if="loading" class="loading">Loading…</div>
    <template v-else>
      <div class="card card-pad">
        <h4 style="font-size:16px;margin-bottom:2px">Document scanning</h4>
        <p style="font-size:13px;color:var(--ink-2);margin-bottom:16px">
          Optional. Drop a vet invoice, receipt, or vaccine record into “Add a record” and
          have it read and pre-filled for you. You bring your own API key, so you own the
          cost and the data-sharing decision — FurFiles works fully without this.
        </p>

        <div class="field">
          <label>Provider</label>
          <div class="provider" data-testid="settings-provider">Anthropic (Claude)</div>
          <p class="hint" style="margin-top:6px">
            The provider FurFiles reads documents with today, so it needs an Anthropic key.
          </p>
        </div>

        <div class="field">
          <label for="set-key">API key</label>
          <input
            id="set-key"
            v-model="form.api_key"
            type="password"
            autocomplete="off"
            data-testid="settings-api-key"
            :placeholder="hasKey ? '•••••••••• saved — leave blank to keep it' : 'sk-ant-…'"
          />
          <p v-if="keyError" class="hint bad" data-testid="settings-key-error">{{ keyError }}</p>
          <p v-else class="hint" style="margin-top:6px">
            Stored encrypted on your server, never shown again, never logged.
          </p>
        </div>

        <div class="field">
          <label for="set-model">Model</label>
          <template v-if="models.length">
            <select id="set-model" v-model="choice" data-testid="settings-model-select">
              <option v-for="m in models" :key="m.id" :value="m.id">{{ m.display_name }}</option>
              <option :value="CUSTOM">Custom…</option>
            </select>
            <input
              v-if="choice === CUSTOM"
              v-model="form.model"
              class="custom-model"
              placeholder="Model ID, e.g. claude-sonnet-5"
              aria-label="Custom model ID"
              data-testid="settings-model-custom"
            />
          </template>
          <input
            v-else
            id="set-model"
            v-model="form.model"
            placeholder="claude-sonnet-5"
            data-testid="settings-model-custom"
          />
          <p v-if="modelsLoading" class="hint" data-testid="settings-model-note">Loading the models your key can use…</p>
          <p v-else-if="modelsError" class="hint bad" data-testid="settings-model-note">
            Couldn't load the model list. {{ modelsError }} Type a model ID instead.
          </p>
          <p v-else-if="!models.length && !keyError" class="hint" data-testid="settings-model-note">
            Paste your API key to choose from the models it can use, or type a model ID.
          </p>
        </div>

        <label class="toggle">
          <input type="checkbox" v-model="form.enabled" :disabled="!hasKey && !form.api_key.trim()" />
          <span>Enable document scanning</span>
        </label>

        <p class="privacy" style="margin-top:14px">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
          <span>When you scan a document, that file is sent to the AI model you configure above.
            Extracted fields always pre-fill for your review — a record is never saved without you.</span>
        </p>

        <p v-if="error" style="color:var(--crit);font-size:13px;margin:12px 0 0">{{ error }}</p>
        <p v-if="saved" style="color:var(--good);font-size:13px;margin:12px 0 0">Saved.</p>

        <div class="setbtns">
          <button class="btn primary" :disabled="saving || keyRefused" @click="save">
            {{ saving ? 'Saving…' : 'Save' }}
          </button>
          <button v-if="hasKey" class="btn sec" :disabled="saving" @click="disconnect">
            Remove key
          </button>
        </div>
      </div>

      <div class="card card-pad" style="margin-top:16px" data-testid="display-settings">
        <h4 style="font-size:16px;margin-bottom:2px">Display</h4>
        <p style="font-size:13px;color:var(--ink-2);margin-bottom:12px">
          Horses always read as stallion, mare and gelding. Dogs and cats use plain words
          unless you prefer the ones breeders use.
        </p>
        <label class="toggle">
          <input
            type="checkbox"
            data-testid="breeder-terms-toggle"
            :checked="display.breeder_terms"
            :disabled="displaySaving"
            @change="setBreederTerms(($event.target as HTMLInputElement).checked)"
          />
          <span>Use breeder terms <small style="color:var(--ink-3);font-weight:500">(dog / bitch, tom / queen)</small></span>
        </label>
        <p v-if="displayError" style="color:var(--crit);font-size:13px;margin:12px 0 0">{{ displayError }}</p>
      </div>
    </template>
  </div>
</template>

<style scoped>
/* Read-only, so it must not look like a control that failed to open (#67). */
.provider { font-size: 15px; font-weight: 700; color: var(--ink); padding: 2px 0; }
.custom-model { margin-top: 8px; }
.hint.bad { color: var(--crit); margin-top: 6px; }
</style>
