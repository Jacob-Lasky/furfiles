// Minimal cross-view signal. No full store yet — the app is small and views
// re-fetch on mount. The one case that needs a nudge: adding a record while
// already ON a pet's profile navigates to the same route (no remount), so the
// timeline wouldn't refresh. Bumping this ref lets a mounted view reload itself.
import { ref, reactive, shallowRef, watchEffect, onBeforeUnmount } from 'vue'
import { api } from './api'
import type { DisplaySettings } from './types'

export const dataVersion = ref(0)
export function bumpData(): void { dataVersion.value += 1 }

// Household display preferences (#72), read by every surface that shows a
// pet's sex. Loaded once at startup; Settings writes through `setDisplay`.
// Until (or unless) the load succeeds the defaults apply: plain words.
export const display = reactive<DisplaySettings>({ breeder_terms: false })
// Bumped by every write, so a startup GET that lands AFTER a save can't put the
// old value back over the one the server just confirmed.
let displayWrites = 0
export async function loadDisplay(): Promise<void> {
  const before = displayWrites
  try {
    const d = await api.displaySettings()
    if (displayWrites === before) display.breeder_terms = d.breeder_terms
  } catch (_) { /* keep the defaults */ }
}
export function setDisplay(d: DisplaySettings): void {
  displayWrites += 1
  display.breeder_terms = d.breeder_terms
}

// The floating + is a speed-dial and the ONE place for actions (#77). The shell
// owns the actions that make sense everywhere (add a record, import); a view
// contributes its own ("Edit Pedro", "Add a pet") while it is mounted. The
// action runs inside the view, so the sheet it opens — and any hand-off from it,
// like PetSheet → ArchiveSheet — stays owned by the view, not the shell.
export type DialIcon = 'record' | 'import' | 'edit' | 'pet' | 'contact'
export interface DialAction {
  key: string // stable; the item's testid is `dial-<key>`
  label: string
  icon: DialIcon
  run: () => void
}
export const pageActions = shallowRef<DialAction[]>([])

// Register this view's dial actions for as long as it is mounted. `build` is
// reactive, so a label that waits on data ("Edit <name>") fills in once loaded.
export function usePageActions(build: () => DialAction[]): void {
  let mine: DialAction[] | null = null
  const stop = watchEffect(() => { mine = build(); pageActions.value = mine })
  onBeforeUnmount(() => {
    stop()
    // Only clear what this view set: never wipe actions a newer view registered.
    if (pageActions.value === mine) pageActions.value = []
  })
}
