<script lang="ts">
  import type { Snippet } from 'svelte'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import { t } from '$lib/i18n'
  import type { HostDevice } from '../../device/hostContext'
  let { customTitle, title, subtitle, device, back, modal = false, switchDevice, action }: {
    customTitle?: Snippet | null; title: string; subtitle?: string; device?: HostDevice | null; back?: () => void; modal?: boolean; switchDevice?: () => void; action?: Snippet | null
  } = $props()
</script>

<header class="flex items-center gap-2 px-4 py-2">
  {#if back}<button class="mobile-icon mobile-glass" aria-label={$t(modal ? 'common.close' : 'mobile.back')} onclick={back}>{#if modal}<XIcon />{:else}<CaretLeftIcon />{/if}</button>{/if}
  {#snippet heading()}
    {#if device}<span class="flex items-center gap-1 text-xs text-muted-foreground"><span class={['size-2 rounded-full',device.state === 'online' ? 'bg-green-600' : 'bg-muted-foreground']}></span>{device.name}{#if switchDevice}<CaretDownIcon />{/if}</span>{/if}
    <span class={['block truncate font-semibold text-heading', device ? 'text-[22px]' : 'text-lg']}>{title}</span>
    {#if subtitle}<small class="block truncate text-muted-foreground">{subtitle}</small>{/if}
  {/snippet}
  {#if customTitle}{@render customTitle()}{:else if switchDevice}<button class="min-h-11 min-w-0 text-left" onclick={switchDevice}>{@render heading()}</button>
  {:else}<h1 class={['min-w-0', back && 'mobile-glass rounded-full px-4 py-2']}>{@render heading()}</h1>{/if}
  <div class="ml-auto flex shrink-0">{@render action?.()}</div>
</header>
