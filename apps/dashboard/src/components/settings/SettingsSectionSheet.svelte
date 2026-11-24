<script lang="ts">
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { t } from '$lib/i18n'
  import { dashboardPath } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Separator } from '$lib/components/ui/separator/index.js'
  import ChatCircleIcon from 'phosphor-svelte/lib/ChatCircleIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { isSettingsSectionActive, settingsSections, type SettingsSection } from './sections'

  let { open = $bindable(false), currentPath, deviceName, onBackToChat }: {
    open?: boolean
    currentPath: string
    deviceName: string | null
    onBackToChat: () => void
  } = $props()

  const optionClass = 'flex h-13 w-full items-center gap-3.5 rounded-lg px-3 text-left text-[0.9375rem] text-foreground outline-none hover:bg-muted focus-visible:bg-muted [&_svg]:size-5 [&_svg]:shrink-0'

  function activate(event: MouseEvent, section: SettingsSection): void {
    event.preventDefault()
    open = false
    void navigate(section.path)
  }

  function backToChat(): void {
    open = false
    onBackToChat()
  }
</script>

<Sheet.Root bind:open>
  <BottomSheet dismiss={() => open = false} showCloseButton={false} class="px-3">
    <div class="flex h-11 items-center justify-between pr-1 pl-2">
      <div class="min-w-0">
        <Sheet.Title class="text-base font-semibold text-heading">{$t('settings.title')}</Sheet.Title>
        <Sheet.Description class={deviceName ? 'truncate text-xs text-muted-foreground' : 'sr-only'}>{deviceName ?? $t('settings.switchSection')}</Sheet.Description>
      </div>
      <Sheet.Close>
        {#snippet child({ props })}
          <Button {...props} variant="ghost" size="icon" class="size-11 [&_svg:not([class*=size-])]:size-5" aria-label={$t('common.close')}><XIcon /></Button>
        {/snippet}
      </Sheet.Close>
    </div>
    <nav aria-label={$t('settings.sections')}>
      {#each settingsSections as section (section.path)}
        {@const active = isSettingsSectionActive(section, currentPath)}
        <a
          href={dashboardPath(section.path)}
          aria-current={active ? 'page' : undefined}
          onclick={(event) => activate(event, section)}
          class={[optionClass, active ? 'bg-selected font-medium text-heading hover:bg-selected [&_svg]:text-primary' : '[&_svg]:text-muted-foreground']}
        >
          <section.icon />
          <span class="min-w-0 truncate">{$t(section.label)}</span>
          {#if active}<CheckIcon class="ml-auto" aria-hidden="true" />{/if}
        </a>
      {/each}
    </nav>
    <Separator class="mx-1 my-2 data-[orientation=horizontal]:w-auto" />
    <button type="button" class={[optionClass, '[&_svg]:text-muted-foreground']} onclick={backToChat}>
      <ChatCircleIcon />{$t('nav.backToChat')}
    </button>
  </BottomSheet>
</Sheet.Root>
