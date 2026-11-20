<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Snippet } from 'svelte'
  import { base } from '$app/paths'
  import GearIcon from 'phosphor-svelte/lib/GearIcon'
  import QuestionIcon from 'phosphor-svelte/lib/QuestionIcon'
  import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon'
  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon'
  import { dashboardPath, dashboardRelativePath } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import ShortcutKeys from '$lib/components/shortcuts/ShortcutKeys.svelte'
  import { chatShortcuts } from '$lib/shortcuts/definitions'
  import { formatShortcutKeys } from '$lib/shortcuts/format'
  import DeviceSessionList from './DeviceSessionList.svelte'
  import DeviceBotList from '../bots/DeviceBotList.svelte'
  import { isSettingsPath, isSettingsSectionActive, settingsSections } from '../settings/sections'

  let { groups, deviceSwitcher, onNewChat = () => void navigate('/'), onSettings = () => void navigate('/settings/common'), settingsDisabled = false }: {
    groups?: Snippet
    deviceSwitcher?: Snippet
    onNewChat?: () => void
    onSettings?: () => void
    settingsDisabled?: boolean
  } = $props()
  const sidebar = Sidebar.useSidebar()
  let currentPath = $state(dashboardRelativePath())
  const settingsPath = $derived(isSettingsPath(currentPath))

  function openSection(event: MouseEvent, path: string): void {
    event.preventDefault()
    void navigate(path)
  }
</script>

<!-- The phone sidebar covers the whole screen, so close it once a choice navigates away. -->
<svelte:window onpopstate={() => { currentPath = dashboardRelativePath(); sidebar.setOpenMobile(false) }} />

<Sidebar.Root collapsible="icon">
  <Sidebar.Header class="group/brand relative h-20 shrink-0 flex-row items-center gap-2 px-4 py-0 md:h-14 md:gap-1 md:px-2">
    <div
      class="flex h-full min-w-0 flex-1 items-center gap-3 px-2 text-sm font-semibold text-heading group-data-[collapsible=icon]:px-0 group-data-[collapsible=icon]:group-hover/brand:invisible group-data-[collapsible=icon]:group-has-[:focus-visible]/brand:invisible"
    >
      <span class="flex size-8 shrink-0 items-center justify-center">
        <img src={`${base}/logo.svg`} alt="" class="size-8 shrink-0 object-contain" />
      </span>
      <span class="truncate group-data-[collapsible=icon]:hidden">Pontia</span>
    </div>
    <Sidebar.Trigger class="size-12 shrink-0 [&_svg:not([class*=size-])]:size-6 md:size-7 md:[&_svg:not([class*=size-])]:size-4 group-data-[collapsible=icon]:absolute group-data-[collapsible=icon]:inset-0 group-data-[collapsible=icon]:m-auto group-data-[collapsible=icon]:size-8 group-data-[collapsible=icon]:opacity-0 group-data-[collapsible=icon]:group-hover/brand:opacity-100 group-data-[collapsible=icon]:focus-visible:opacity-100" />
  </Sidebar.Header>
  <Sidebar.Separator class="mx-4 data-[orientation=horizontal]:w-auto group-data-[collapsible=icon]:mx-2" />
  <!-- Primary actions stay in place; everything below them scrolls as one list. -->
  <div class="flex shrink-0 flex-col">
    {#if settingsPath}
      <Sidebar.Group>
        <Sidebar.GroupContent>
          <Sidebar.Menu>
            <Sidebar.MenuItem>
              <Sidebar.MenuButton class="h-9 text-[13px] text-sidebar-foreground/70" tooltipContent={$t('nav.backToChat')} onclick={onNewChat}>
                <ArrowLeftIcon />
                <span>{$t('nav.backToChat')}</span>
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
      {@render deviceSwitcher?.()}
    {:else}
      <Sidebar.Group>
        <Sidebar.GroupContent>
          <Sidebar.Menu>
            <Sidebar.MenuItem>
              <Sidebar.MenuButton
                isActive={currentPath === '/'}
                class="mb-2 h-10 justify-center bg-primary-solid text-[13px] font-semibold text-primary-foreground hover:bg-primary-solid-hover hover:text-primary-foreground active:bg-primary-solid-active active:text-primary-foreground data-[active=true]:bg-primary-solid data-[active=true]:text-primary-foreground data-[active=true]:hover:bg-primary-solid-hover group-data-[collapsible=icon]:mb-0 group-data-[collapsible=icon]:bg-transparent! group-data-[collapsible=icon]:text-sidebar-foreground! group-data-[collapsible=icon]:hover:bg-sidebar-accent! group-data-[collapsible=icon]:hover:text-sidebar-accent-foreground!"
                tooltipContent={$t('nav.newChat')}
                onclick={onNewChat}
              >
                <NotePencilIcon />
                <span class="group-data-[collapsible=icon]:hidden">{$t('nav.newChat')}</span>
                <ShortcutKeys keys={chatShortcuts.newChat.keys} class="ml-auto shrink-0 border border-primary-foreground/40 bg-transparent px-1 py-0 text-[10px] text-primary-foreground/80 group-data-[collapsible=icon]:hidden" aria-hidden="true" />
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
      {@render deviceSwitcher?.()}
    {/if}
  </div>
  <Sidebar.Content class="overflow-x-hidden overflow-y-auto group-data-[collapsible=icon]:overflow-y-auto">
    {#if settingsPath}
      <Sidebar.Group>
        <Sidebar.GroupLabel class="h-8 px-2">{$t('settings.title')}</Sidebar.GroupLabel>
        <Sidebar.GroupContent>
          <Sidebar.Menu aria-label={$t('settings.sections')}>
            {#each settingsSections as section (section.path)}
              {@const active = isSettingsSectionActive(section, currentPath)}
              <Sidebar.MenuItem>
                <Sidebar.MenuButton isActive={active} tooltipContent={$t(section.label)}>
                  {#snippet child({ props })}
                    <a {...props} href={dashboardPath(section.path)} aria-current={active ? 'page' : undefined} onclick={(event) => openSection(event, section.path)}>
                      <section.icon />
                      <span>{$t(section.label)}</span>
                    </a>
                  {/snippet}
                </Sidebar.MenuButton>
              </Sidebar.MenuItem>
            {/each}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
    {:else if groups}
      {@render groups()}
    {:else}
      <DeviceBotList />
      <DeviceSessionList />
    {/if}
  </Sidebar.Content>
  <Sidebar.Footer>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton
          class="h-9 px-3 text-[13px]"
          tooltipContent={$t('settings.title')}
          disabled={settingsDisabled}
          onclick={onSettings}
        >
          <GearIcon />
          <span>{$t('settings.title')}</span>
        </Sidebar.MenuButton>
        <Sidebar.MenuAction
          class="peer-data-[size=default]/menu-button:top-2 hidden md:flex"
          aria-label={$t('shortcuts.title')}
          title={$t('shortcuts.withKeys', { keys: formatShortcutKeys(chatShortcuts.showHelp.keys) })}
          onclick={() => document.dispatchEvent(new CustomEvent('pontia:open-chat-shortcuts'))}
        >
          <QuestionIcon />
        </Sidebar.MenuAction>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Footer>
  <Sidebar.Rail />
</Sidebar.Root>
