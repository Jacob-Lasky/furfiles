<script setup lang="ts">
// The floating + as a speed-dial (#77): ONE place for actions. Tapping it fans
// out labelled pills from the bottom right; tap outside, Escape, or the button
// again closes it and the + turns back from ×. It only TRIGGERS — each action
// opens the same sheet or page it always did.
//
// A menu button (WAI-ARIA APG): opening moves focus to the action nearest the
// button (the primary one), arrows/Home/End move between actions, Escape closes
// and returns focus to the button, and Tab closes and lets focus move on as the
// browser would (the + is next in the tab order, after the menu).
import { ref, nextTick, watch, onBeforeUnmount } from 'vue'
import { useRoute } from 'vue-router'
import type { DialAction, DialIcon } from '../store'
import { PAW_PATH } from '../format'

// Listed top to bottom; the LAST action sits right above the button and is the
// one focus lands on, so callers put the most common action last.
const props = defineProps<{ actions: DialAction[] }>()

const open = ref(false)
const trigger = ref<HTMLButtonElement | null>(null)
// Function refs by index: a v-for array ref does not promise source order, and
// arrow-key order must match what is on screen.
const items: HTMLButtonElement[] = []
function setItem(i: number, el: unknown) { if (el) items[i] = el as HTMLButtonElement }
function list(): HTMLButtonElement[] { return items.slice(0, props.actions.length) }
const route = useRoute()

const ICONS: Record<Exclude<DialIcon, 'pet'>, string> = {
  record: 'M14 3H7a2 2 0 00-2 2v14a2 2 0 002 2h10a2 2 0 002-2V8zM14 3v5h5M12 11v6M9 14h6',
  import: 'M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M17 8l-5-5-5 5M12 3v12',
  edit: 'M12 20h9M16.5 3.5a2.1 2.1 0 013 3L7 19l-4 1 1-4z',
  contact: 'M16 21v-2a4 4 0 00-4-4H6a4 4 0 00-4 4v2M9 11a4 4 0 100-8 4 4 0 000 8zM19 8v6M22 11h-6',
}

function focusItem(i: number) {
  const els = list()
  if (els.length) els[(i + els.length) % els.length].focus()
}

async function show() {
  open.value = true
  await nextTick()
  focusItem(list().length - 1)
}
function close(refocus = true) {
  if (!open.value) return
  open.value = false
  if (refocus) trigger.value?.focus({ preventScroll: true })
}
function toggle() { if (open.value) close(); else show() }

function choose(a: DialAction) {
  close()
  a.run()
}

function onMenuKey(e: KeyboardEvent) {
  const at = list().indexOf(document.activeElement as HTMLButtonElement)
  if (e.key === 'ArrowDown') focusItem(at + 1)
  else if (e.key === 'ArrowUp') focusItem(at - 1)
  else if (e.key === 'Home') focusItem(0)
  else if (e.key === 'End') focusItem(list().length - 1)
  else if (e.key === 'Tab') { close(false); return } // let the browser move focus
  else return
  e.preventDefault()
}
// Escape works wherever focus is while the dial is open (the button included).
function onDocKey(e: KeyboardEvent) {
  if (e.key === 'Escape') { e.preventDefault(); close() }
}
watch(open, (o) => {
  if (o) document.addEventListener('keydown', onDocKey)
  else document.removeEventListener('keydown', onDocKey)
})
onBeforeUnmount(() => document.removeEventListener('keydown', onDocKey))
// Navigating away (Back, a link) must not leave the dial open on the new page,
// nor focus stranded in its now-hidden menu.
watch(() => route.fullPath, () => close())
</script>

<template>
  <div class="dial" :class="{ open }">
    <div v-if="open" class="dial-scrim" data-testid="dial-scrim" @click="close()"></div>
    <ul
      id="speed-dial-menu" class="dial-menu" role="menu" aria-label="Quick actions"
      data-testid="dial-menu" @keydown="onMenuKey"
    >
      <li
        v-for="(a, i) in props.actions" :key="a.key" role="none"
        :style="{ '--i': props.actions.length - 1 - i }"
      >
        <button
          :ref="(el) => setItem(i, el)" type="button" role="menuitem" tabindex="-1" class="dial-item"
          :data-testid="`dial-${a.key}`" @click="choose(a)"
        >
          <span class="dial-label">{{ a.label }}</span>
          <span class="dial-ico" aria-hidden="true">
            <svg v-if="a.icon === 'pet'" viewBox="0 0 24 24" fill="currentColor"><path :d="PAW_PATH" /></svg>
            <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path :d="ICONS[a.icon]" /></svg>
          </span>
        </button>
      </li>
    </ul>
    <button
      ref="trigger" type="button" class="fab" data-testid="fab"
      aria-label="Quick actions" aria-haspopup="menu" :aria-expanded="open" aria-controls="speed-dial-menu"
      @click="toggle"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4"><path d="M12 5v14M5 12h14"/></svg>
    </button>
  </div>
</template>
