<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import { registerShortcut } from '$lib/shortcuts/dispatch'
  import { shortcutIds, registry } from '$lib/shortcuts/registry'
  import { messageInputShortcuts } from '$lib/shortcuts/definitions'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import ShortcutKeys from './ShortcutKeys.svelte'
  import { dashboardPath } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'
  let { scope }: { scope?: string } = $props()
  const path = '/settings/shortcuts'
  const settingsHref = $derived(scope ? scope + path : dashboardPath(path))
  function customize(event: MouseEvent) {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
    event.preventDefault()
    shortcutsOpen = false
    void navigate(path, undefined, {scope})
  }
  let shortcutsOpen = $state(false)
  const open = () => { shortcutsOpen = true }
  registerShortcut('showHelp', open)
  onMount(() => {
    document.addEventListener('pontia:open-chat-shortcuts', open)
    return () => document.removeEventListener('pontia:open-chat-shortcuts', open)
  })
</script>

<Dialog.Root bind:open={shortcutsOpen}>
  <Dialog.Content class="max-h-[90svh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{$t('shortcuts.title')}</Dialog.Title>
      <Dialog.Description>{$t('shortcuts.description')}</Dialog.Description>
    </Dialog.Header>

    <div class="space-y-1">
      {#each shortcutIds as id (id)}
        <div class="flex items-center justify-between gap-4 py-1.5">
          <div class="min-w-0 text-sm">{$t(registry[id].label)}</div>
          <ShortcutKeys shortcutId={id} class="inline-flex shrink-0 border border-border bg-transparent" />
        </div>
      {/each}
    </div>

    <section class="space-y-1 border-t pt-4" aria-labelledby="message-input-shortcuts-title">
      <h3 id="message-input-shortcuts-title" class="font-medium">{$t('shortcuts.messageInput')}</h3>
      {#each messageInputShortcuts as shortcut}
        <div class="flex items-center justify-between gap-4 py-1.5">
          <div class="min-w-0 text-sm">{$t(shortcut.label)}</div>
          <div class="flex flex-wrap justify-end gap-2">
            {#each shortcut.keyGroups as keys}
              <ShortcutKeys {keys} class="inline-flex shrink-0 border border-border bg-transparent" />
            {/each}
          </div>
        </div>
      {/each}
      <p class="text-xs text-muted-foreground">{$t('shortcuts.inputSuggestions')}</p>
    </section>
    <a class="justify-self-end text-sm font-medium underline underline-offset-4" href={settingsHref} onclick={customize}>{$t('shortcuts.customize')}</a>
  </Dialog.Content>
</Dialog.Root>
