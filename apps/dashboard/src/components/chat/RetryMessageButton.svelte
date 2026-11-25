<script lang="ts">
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { buttonVariants } from '$lib/components/ui/button/index.js'
  import * as Tooltip from '$lib/components/ui/tooltip/index.js'
  import { t } from '$lib/i18n'

  let { label, reason, disabled, onclick }: {
    label?: string
    reason?: string | null
    disabled: boolean
    onclick: () => void
  } = $props()
</script>

<Tooltip.Provider>
  <Tooltip.Root>
    <Tooltip.Trigger {disabled} {onclick} aria-label={label ?? $t('chat.resendFailed')}
      class={buttonVariants({ variant: 'ghost', size: 'icon', class: 'self-center rounded-full text-destructive hover:bg-destructive/10 hover:text-destructive focus-visible:ring-destructive/50 max-md:size-11' })}>
      <WarningCircleIcon weight="fill" class="size-5" aria-hidden="true" />
    </Tooltip.Trigger>
    <Tooltip.Content role="tooltip" class="max-w-64 break-words">
      {reason ?? $t('bots.sendFailed')} · {$t('chat.resendFailed')}
    </Tooltip.Content>
  </Tooltip.Root>
</Tooltip.Provider>
