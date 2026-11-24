<script lang="ts">
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
  import { t } from '$lib/i18n';
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import type { SessionView } from '../../api/types'

  interface Props {
    open: boolean
    session: SessionView | null
    busy?: boolean
    error?: string | null
    onConfirm: (title: string | null) => void
    onCancel?: () => void
  }

  let { open = $bindable(false), session, busy = false, error = null, onConfirm, onCancel }: Props = $props()
  let title = $state('')
  let editingSessionId = $state<string | null>(null)

  $effect(() => {
    if (open && session?.session_id !== editingSessionId) {
      editingSessionId = session?.session_id ?? null
      title = session ? (session.title ?? sessionChatTitle(session, $t)) : ''
    }
    if (!open) editingSessionId = null
  })

  function cancel(): void {
    open = false
    onCancel?.()
  }

  function submit(): void {
    if (!session || busy) return
    onConfirm(title.trim() || null)
  }
</script>

<Dialog.Root bind:open>
  <ResponsiveDialog mode="sheet" dismiss={cancel} class="sm:max-w-md">
    <form onsubmit={(event) => { event.preventDefault(); submit() }}>
      <Dialog.Header>
        <Dialog.Title>{$t('chat.rename')}</Dialog.Title>
        <Dialog.Description>
          {$t('chat.renameDescription')}
        </Dialog.Description>
      </Dialog.Header>

      <div class="mt-4 grid gap-2">
        <Label for="rename-session-title">{$t('chat.sessionTitle')}</Label>
        <Input id="rename-session-title" bind:value={title} placeholder={session ? sessionChatTitle(session, $t) : $t('chat.sessionTitle')} disabled={busy || !session} />
        {#if error}
          <p class="text-sm text-destructive">{error}</p>
        {/if}
      </div>

      <Dialog.Footer class="mt-5">
        <Button type="button" variant="outline" onclick={cancel} disabled={busy}>{$t('common.cancel')}</Button>
        <Button type="submit" disabled={busy || !session}>{busy ? $t('common.saving') : $t('chat.rename')}</Button>
      </Dialog.Footer>
    </form>
  </ResponsiveDialog>
</Dialog.Root>
