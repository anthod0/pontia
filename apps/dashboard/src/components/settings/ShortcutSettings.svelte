<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import ShortcutKeys from '$lib/components/shortcuts/ShortcutKeys.svelte'
  import { t, type MessageKey } from '$lib/i18n'
  import { comboKeys, eventCombo, ignoredEvent, recordError, registry, shortcutIds, type ShortcutId, type KeyCombo } from '$lib/shortcuts/registry'
  import { formatShortcutKeys } from '$lib/shortcuts/format'
  import { shortcuts } from '$lib/shortcuts/store.svelte'

  let recording = $state<ShortcutId | null>(null)
  let error = $state<MessageKey | null>(null)
  let pending = $state<{ id: ShortcutId; combo: KeyCombo } | null>(null)
  const conflicts = $derived(pending ? shortcuts.conflicts(pending.id, pending.combo) : [])

  function start(id: ShortcutId) {
    pending = null
    error = null
    recording = id
  }

  function propose(id: ShortcutId, combo: KeyCombo | null) {
    recording = null
    pending = null
    if (combo && shortcuts.conflicts(id, combo).length) pending = { id, combo }
    else shortcuts.assign(id, combo)
  }

  function record(id: ShortcutId, event: KeyboardEvent) {
    if (recording !== id || ignoredEvent(event)) return
    if (event.code === 'Tab') return
    event.preventDefault()
    event.stopPropagation()
    if (event.repeat) return
    if (event.code === 'Escape') {
      recording = null
      return
    }
    if (event.code === 'Backspace' || event.code === 'Delete') {
      propose(id, null)
      return
    }
    const combo = eventCombo(event)
    error = recordError(id, combo)
    if (!error) propose(id, combo)
  }

  function resetAll() {
    recording = null
    pending = null
    shortcuts.reset()
  }

  function replace() {
    if (!pending) return
    shortcuts.assign(pending.id, pending.combo, conflicts)
    pending = null
  }
</script>

<section aria-labelledby="shortcut-settings-title" class="mx-auto max-w-4xl space-y-5">
  <header class="flex items-center justify-between gap-4">
    <h1 id="shortcut-settings-title" class="text-xl font-semibold tracking-tight">{$t('settings.shortcuts')}</h1>
    <Button variant="outline" size="sm" disabled={!shortcutIds.some(id => shortcuts.changed(id))}
      onclick={resetAll}>{$t('shortcuts.resetAll')}</Button>
  </header>
  <div class="divide-y rounded-item border bg-card px-5">
    {#each shortcutIds as id (id)}
      {@const action = registry[id]}
      <div class="py-3">
        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-sm font-medium">{$t(action.label)}</div>
            <div class="text-xs text-muted-foreground">{$t(`shortcuts.scope.${action.scope}`)}</div>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            {#if shortcuts.changed(id)}
              <Button variant="ghost" size="sm" aria-label={$t('shortcuts.resetAction', { action: $t(action.label) })}
                onclick={() => propose(id, action.combo)}>{$t('shortcuts.reset')}</Button>
            {/if}
            {#if id === 'openChatByPosition'}
              <ShortcutKeys shortcutId={id} aria-hidden="true" />
              <Switch aria-label={$t(action.label)} bind:checked={() => !!shortcuts.binding(id), checked => propose(id, checked ? action.combo : null)} />
            {:else}
              <Button size="sm" variant={recording === id ? 'secondary' : 'outline'} class="min-w-24 font-mono"
                aria-label={$t('shortcuts.recordAction', { action: $t(action.label) })} aria-pressed={recording === id}
                onclick={() => start(id)} onblur={() => { if (recording === id) recording = null }}
                onkeydown={event => record(id, event)}>
                {recording === id ? $t('shortcuts.listening') : shortcuts.label(id) || $t('common.disabled')}
              </Button>
            {/if}
          </div>
        </div>
        {#if recording === id}
          <p role="status" class="mt-2 text-sm text-muted-foreground">{$t(error ?? 'shortcuts.recordHint')}</p>
        {:else if pending?.id === id}
          <div class="mt-3 flex flex-wrap items-center gap-2 rounded-item border bg-muted/40 px-3 py-2" role="status">
            <span class="mr-auto text-sm">{$t('shortcuts.conflict', { keys: formatShortcutKeys(comboKeys(pending.combo)), actions: conflicts.map(other => $t(registry[other].label)).join(', ') })}</span>
            <Button size="sm" onclick={replace}>{$t('shortcuts.replace')}</Button>
            <Button size="sm" variant="ghost" onclick={() => pending = null}>{$t('common.cancel')}</Button>
          </div>
        {/if}
      </div>
    {/each}
  </div>
</section>
