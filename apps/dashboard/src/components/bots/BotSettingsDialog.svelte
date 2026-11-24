<script lang="ts">
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
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
  <ResponsiveDialog mode="fullscreen" class="max-md:overflow-hidden sm:max-w-md" aria-describedby={undefined}>
    <Dialog.Header class="shrink-0 pr-11"><Dialog.Title>{$t(bot ? 'bots.edit' : 'bots.create')}</Dialog.Title></Dialog.Header>
    <div class="grid min-h-0 gap-4 max-md:overflow-y-auto max-md:overscroll-contain">
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
    </div>
  </ResponsiveDialog>
</Dialog.Root>

{#if deleting && bot}
  <BotDeleteDialog {bot} onclose={() => { deleting = false }} ondone={() => { deleting = false; onclose() }} />
{/if}
