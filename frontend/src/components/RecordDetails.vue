<script setup lang="ts">
// The shared "core details" of a record: the scanned-document badge, the
// markdown notes, the cost-breakdown table, and custom (extra) field chips.
// Rendered IDENTICALLY on the pet timeline (Pet.vue) and in the read-only record
// view (RecordSheet, #44) so the two surfaces can't drift. Styling is global
// (.aichip / .mdnotes / .breakdown / .kvchips in styles.css) — the single source
// for how these read. Notes render as markdown via renderMarkdown, which is
// ESCAPE-FIRST (the source is untrusted user/LLM content).
import { computed } from 'vue'
import { money, renderMarkdown } from '../format'
import type { RecordLike } from '../types'
const props = defineProps<{ record: RecordLike }>()
const hasExtra = computed(() => !!props.record.extra && Object.keys(props.record.extra).length > 0)
</script>

<template>
  <div v-if="record.source === 'scan'"><span class="aichip"><svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2l2.2 6.5L21 10l-6.8 1.5L12 18l-2.2-6.5L3 10l6.8-1.5z"/></svg>from a scanned document</span></div>
  <div v-if="record.notes" class="mdnotes" v-html="renderMarkdown(record.notes)"></div>
  <div v-if="record.line_items && record.line_items.length" class="breakdown">
    <div v-for="(li, i) in record.line_items" :key="i" class="bd-row">
      <span>{{ li.description }}</span><span class="mono">{{ money(li.amount_cents) }}</span>
    </div>
  </div>
  <div v-if="hasExtra" class="kvchips">
    <span v-for="(v, k) in record.extra" :key="k" class="kvchip"><b>{{ k }}</b>{{ v }}</span>
  </div>
</template>
