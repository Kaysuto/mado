import { type Item, createNote, listItems } from './api'

/** The note that is open in the Notes space. Kept here so the space reopens on it. */
export const notes = $state<{ selected: number | null }>({ selected: null })

/** Opens the note about `item`, starting one when there is none yet. */
export async function takeNote(item: Item) {
  const [existing] = await listItems({ kinds: ['note'], about: item.id, limit: 1 })
  notes.selected = existing?.id ?? (await createNote('', item.id))
}
