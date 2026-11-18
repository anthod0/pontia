<script lang="ts">
  import type { BotAvatarColor, BotAvatarShape } from '../../api/types'
  import { t } from '$lib/i18n'
  import * as ToggleGroup from '$lib/components/ui/toggle-group/index.js'
  import BotAvatar from './BotAvatar.svelte'
  import { avatarColors, colorOptions, shapeOptions } from './avatar'

  let { shape = $bindable(), color = $bindable(), disabled = false }: {
    shape: BotAvatarShape
    color: BotAvatarColor
    disabled?: boolean
  } = $props()
</script>

<fieldset class="grid min-w-0 gap-3" {disabled}>
  <legend class="mb-2 text-sm font-medium">{$t('bots.avatar')}</legend>
  <BotAvatar {shape} {color} class="mx-auto size-16" />
  <ToggleGroup.Root type="single" bind:value={() => shape, (value) => { if (value) shape = value as BotAvatarShape }}
    {disabled} spacing={1} class="grid w-full grid-cols-9 gap-1" aria-label={$t('bots.avatarShape')}>
    {#each shapeOptions as option (option)}
      <ToggleGroup.Item value={option} class="h-9 w-full min-w-0 p-0 data-[state=on]:ring-2 data-[state=on]:ring-ring"
        aria-label={$t(`bots.avatarShape.${option}`)} title={$t(`bots.avatarShape.${option}`)}>
        <BotAvatar shape={option} {color} class="size-6" />
      </ToggleGroup.Item>
    {/each}
  </ToggleGroup.Root>
  <ToggleGroup.Root type="single" bind:value={() => color, (value) => { if (value) color = value as BotAvatarColor }}
    {disabled} spacing={1} class="grid w-full grid-cols-9 gap-1" aria-label={$t('bots.avatarColor')}>
    {#each colorOptions as option (option)}
      <ToggleGroup.Item value={option} class="h-9 w-full min-w-0 p-0 data-[state=on]:ring-2 data-[state=on]:ring-ring"
        aria-label={$t(`bots.avatarColor.${option}`)} title={$t(`bots.avatarColor.${option}`)}>
        <span class="size-5 rounded-full" style:background-color={avatarColors[option]}></span>
      </ToggleGroup.Item>
    {/each}
  </ToggleGroup.Root>
</fieldset>
