<script setup lang="ts">
// Shared chrome for the bottom-sheet / centered-modal pattern: the dimmed
// backdrop, the sheet container, the mobile grab handle, and close-on-backdrop.
// Consumers fill the head/body/foot via slots so the chrome lives in one place.
// `testid` stamps a durable probe anchor on the sheet root (#12).
//
// Focus moves INTO the sheet when it opens and back to whatever opened it when
// it closes (#77: the speed-dial hands off to a sheet; focus left on the + sat
// behind the backdrop). The sheet itself takes focus, not its first field: on a
// phone, focusing an input would throw the keyboard up over a sheet the person
// may only have opened to read.
import { ref, onMounted, onBeforeUnmount } from 'vue'

defineProps<{ testid?: string | null }>()
const emit = defineEmits<{ close: [] }>()

const sheet = ref<HTMLElement | null>(null)
let opener: HTMLElement | null = null

onMounted(() => {
  opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
  sheet.value?.focus({ preventScroll: true })
})
onBeforeUnmount(() => {
  // Only if focus is still ours to give back: not when the sheet closed because
  // the person already moved on, or the opener has left the page.
  const inside = !!sheet.value && sheet.value.contains(document.activeElement)
  if (opener && opener.isConnected && (inside || document.activeElement === document.body)) opener.focus({ preventScroll: true })
})
</script>

<template>
  <div class="mback" @click.self="emit('close')">
    <div ref="sheet" class="sheet" :data-testid="testid" tabindex="-1">
      <div class="sheet-grab"></div>
      <slot />
    </div>
  </div>
</template>
