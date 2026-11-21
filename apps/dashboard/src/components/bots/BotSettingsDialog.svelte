<script lang="ts">
  import { t } from '$lib/i18n'
  import type { Bot } from '../../api/types'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'
  import BotSettingsForm from './BotSettingsForm.svelte'
  import BotDeleteDialog from './BotDeleteDialog.svelte'

  let { bot, onclose, onsaved }: { bot?: Bot; onclose: () => void; onsaved: (bot: Bot) => void } = $props()
  let saving = $state(false)
  let deleting = $state(false)
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open && !saving && !deleting) onclose() }}>
  <Dialog.Content class="max-h-[calc(100dvh-2rem)] max-w-[calc(100%-2rem)] overflow-y-auto sm:max-w-md" aria-describedby={undefined}>
    <Dialog.Header><Dialog.Title>{$t(bot ? 'bots.edit' : 'bots.create')}</Dialog.Title></Dialog.Header>
    <BotSettingsForm {bot} bind:saving oncancel={onclose} {onsaved} />
    {#if bot}
      <section class="flex flex-wrap items-center gap-3 border-t pt-4">
        <div class="min-w-0 flex-1 basis-56">
          <h3 class="text-sm font-medium">{$t('bots.deleteSection')}</h3>
          <p class="text-xs text-muted-foreground">{$t('bots.deleteSectionDescription')}</p>
        </div>
        <Button variant="destructive" disabled={saving} onclick={() => { deleting = true }}>
          <TrashIcon class="size-4" />{$t('bots.delete')}
        </Button>
      </section>
    {/if}
  </Dialog.Content>
</Dialog.Root>

{#if deleting && bot}
  <BotDeleteDialog {bot} onclose={() => { deleting = false }} ondone={() => { deleting = false; onclose() }} />
{/if}
