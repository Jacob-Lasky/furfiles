<script setup lang="ts">
import { computed } from 'vue'
import { speciesTrack, PAW_PATH } from '../format'

// The soft portrait watermark (design.md: "a soft paw watermark"), matched to
// the animal. A SINGLE <svg> root is deliberate: per-pet render sites pass
// class="paw", and Vue only forwards fall-through attributes to a single root
// element. A v-if chain of sibling <svg>s is a fragment root, which silently
// drops the class and loses the watermark styling — do not restructure this
// into multiple top-level branches. Only the inner shapes switch on species;
// the unrecognized case reuses the one canonical PAW_PATH.
const props = withDefaults(defineProps<{ species?: string }>(), { species: '' })
const track = computed(() => speciesTrack(props.species))
</script>

<template>
  <svg viewBox="0 0 24 24" fill="currentColor" :data-track="track" aria-hidden="true">
    <!-- Cat print: four small round toes over a rounded pad. -->
    <template v-if="track === 'cat'">
      <ellipse cx="7.6" cy="8" rx="1.7" ry="2.2" />
      <ellipse cx="10.6" cy="6.3" rx="1.7" ry="2.3" />
      <ellipse cx="13.4" cy="6.3" rx="1.7" ry="2.3" />
      <ellipse cx="16.4" cy="8" rx="1.7" ry="2.2" />
      <path d="M12 10.4c-3 0-5.5 2-5.5 4.6 0 2.5 2.6 3.8 5.5 3.8s5.5-1.3 5.5-3.8c0-2.6-2.5-4.6-5.5-4.6z" />
    </template>

    <!-- Dog print: four larger toes in a wider arc over a bigger pad. -->
    <template v-else-if="track === 'dog'">
      <ellipse cx="6.4" cy="8.4" rx="2" ry="2.7" />
      <ellipse cx="10" cy="6" rx="2.1" ry="2.9" />
      <ellipse cx="14" cy="6" rx="2.1" ry="2.9" />
      <ellipse cx="17.6" cy="8.4" rx="2" ry="2.7" />
      <path d="M12 9.6c-3.8 0-6.6 2.6-6.6 5.5 0 2.9 3 4.3 6.6 4.3s6.6-1.4 6.6-4.3c0-2.9-2.8-5.5-6.6-5.5z" />
    </template>

    <!-- Horse: a horseshoe (the track a shod horse leaves), open at the heel. -->
    <template v-else-if="track === 'horse'">
      <path fill-rule="evenodd" d="M4.1 18.5A8.6 8.6 0 1 1 19.9 18.5l-1.4 1a5.7 5.7 0 1 0-13 0l-1.4-1zm2.9-.4a3.9 3.9 0 0 1 10 0l-1.6 1.1a2.2 2.2 0 0 0-6.8 0L7 18.1z" />
    </template>

    <!-- Fish: simple side-view silhouette, body plus tail. -->
    <template v-else-if="track === 'fish'">
      <path d="M20.5 12c-3.8-4.6-10.8-4.6-14.6 0 3.8 4.6 10.8 4.6 14.6 0z" />
      <path d="M5.9 12 2.4 8.4 4.2 12l-1.8 3.6L5.9 12z" />
    </template>

    <!-- Bird track: three forward toes and one back toe. -->
    <path v-else-if="track === 'bird'" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round" d="M12 14.5 6.8 8.2M12 14.5 12 6.2M12 14.5 17.2 8.2M12 14.5 12 20.5" />

    <!-- Fallback: the one canonical paw for any unrecognized species. -->
    <path v-else :d="PAW_PATH" />
  </svg>
</template>
