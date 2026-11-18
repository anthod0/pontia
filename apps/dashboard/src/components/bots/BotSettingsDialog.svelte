<script lang="ts">
  import { t } from '$lib/i18n'
  import type { Bot } from '../../api/types'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import BotSettingsForm from './BotSettingsForm.svelte'

  let { bot, onclose, onsaved }: { bot?: Bot; onclose: () => void; onsaved: (bot: Bot) => void } = $props()
  let saving = $state(false)
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open && !saving) onclose() }}>
  <Dialog.Content class="max-h-[calc(100dvh-2rem)] max-w-[calc(100%-2rem)] overflow-y-auto sm:max-w-md" aria-describedby={undefined}>
    <Dialog.Header><Dialog.Title>{$t(bot ? 'bots.edit' : 'bots.create')}</Dialog.Title></Dialog.Header>
    <BotSettingsForm {bot} bind:saving oncancel={onclose} {onsaved} />
  </Dialog.Content>
</Dialog.Root>
