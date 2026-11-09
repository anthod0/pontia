<script lang="ts">
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
  import { isSettingsPath, isSettingsSectionActive, settingsSections } from '../settings/sections'

  let { groups, deviceSwitcher, onNewChat = () => void navigate('/'), onSettings = () => void navigate('/settings/common'), settingsDisabled = false }: {
    groups?: Snippet
    deviceSwitcher?: Snippet
    onNewChat?: () => void
    onSettings?: () => void
    settingsDisabled?: boolean
  } = $props()
  let currentPath = $state(dashboardRelativePath())
  const settingsPath = $derived(isSettingsPath(currentPath))

  function openSection(event: MouseEvent, path: string): void {
    event.preventDefault()
    void navigate(path)
  }
</script>

<svelte:window onpopstate={() => (currentPath = dashboardRelativePath())} />

<Sidebar.Root collapsible="icon">
  <Sidebar.Header class="group/brand relative h-14 flex-row items-center gap-1 px-2 py-0">
    <div
      class="flex h-full min-w-0 flex-1 items-center gap-3 px-2 text-sm font-semibold text-heading group-data-[collapsible=icon]:px-0 group-data-[collapsible=icon]:group-hover/brand:invisible group-data-[collapsible=icon]:group-has-[:focus-visible]/brand:invisible"
    >
      <span class="flex size-8 shrink-0 items-center justify-center">
        <img src={`${base}/logo.svg`} alt="" class="size-8 shrink-0 object-contain" />
      </span>
      <span class="truncate group-data-[collapsible=icon]:hidden">Pontia</span>
    </div>
    <Sidebar.Trigger class="shrink-0 group-data-[collapsible=icon]:absolute group-data-[collapsible=icon]:inset-0 group-data-[collapsible=icon]:m-auto group-data-[collapsible=icon]:size-8 group-data-[collapsible=icon]:opacity-0 group-data-[collapsible=icon]:group-hover/brand:opacity-100 group-data-[collapsible=icon]:focus-visible:opacity-100" />
  </Sidebar.Header>
  <Sidebar.Separator class="mx-4 data-[orientation=horizontal]:w-auto group-data-[collapsible=icon]:mx-2" />
  <Sidebar.Content class="overflow-hidden">
    {#if settingsPath}
      <Sidebar.Group>
        <Sidebar.GroupContent>
          <Sidebar.Menu>
            <Sidebar.MenuItem>
              <Sidebar.MenuButton class="h-9 text-[13px] text-sidebar-foreground/70" tooltipContent="Back to chat" onclick={onNewChat}>
                <ArrowLeftIcon />
                <span>Back to chat</span>
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
      {@render deviceSwitcher?.()}
      <Sidebar.Group>
        <Sidebar.GroupLabel class="h-8 px-2">Settings</Sidebar.GroupLabel>
        <Sidebar.GroupContent>
          <Sidebar.Menu aria-label="Settings sections">
            {#each settingsSections as section (section.path)}
              {@const active = isSettingsSectionActive(section, currentPath)}
              <Sidebar.MenuItem>
                <Sidebar.MenuButton isActive={active} tooltipContent={section.label}>
                  {#snippet child({ props })}
                    <a {...props} href={dashboardPath(section.path)} aria-current={active ? 'page' : undefined} onclick={(event) => openSection(event, section.path)}>
                      <section.icon />
                      <span>{section.label}</span>
                    </a>
                  {/snippet}
                </Sidebar.MenuButton>
              </Sidebar.MenuItem>
            {/each}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
    {:else}
      <Sidebar.Group>
        <Sidebar.GroupContent>
          <Sidebar.Menu>
            <Sidebar.MenuItem>
              <Sidebar.MenuButton
                isActive={currentPath === '/'}
                class="mb-2 h-10 justify-center bg-primary text-[13px] font-semibold text-primary-foreground hover:bg-primary-hover hover:text-primary-foreground active:bg-primary-active data-[active=true]:bg-primary data-[active=true]:text-primary-foreground data-[active=true]:hover:bg-primary-hover group-data-[collapsible=icon]:mb-0 group-data-[collapsible=icon]:bg-transparent! group-data-[collapsible=icon]:text-sidebar-foreground! group-data-[collapsible=icon]:hover:bg-sidebar-accent! group-data-[collapsible=icon]:hover:text-sidebar-accent-foreground!"
                tooltipContent="New Chat"
                onclick={onNewChat}
              >
                <NotePencilIcon />
                <span class="group-data-[collapsible=icon]:hidden">New Chat</span>
                <ShortcutKeys keys={chatShortcuts.newChat.keys} class="ml-auto shrink-0 border border-primary-foreground/40 bg-transparent px-1 py-0 text-[10px] text-primary-foreground/80 group-data-[collapsible=icon]:hidden" aria-hidden="true" />
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>

      {@render deviceSwitcher?.()}
      {#if groups}{@render groups()}{:else}<DeviceSessionList />{/if}
    {/if}
  </Sidebar.Content>
  <Sidebar.Footer>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton
          class="h-9 px-3 text-[13px]"
          tooltipContent="Settings"
          disabled={settingsDisabled}
          onclick={onSettings}
        >
          <GearIcon />
          <span>Settings</span>
        </Sidebar.MenuButton>
        <Sidebar.MenuAction
          class="peer-data-[size=default]/menu-button:top-2 hidden md:flex"
          aria-label="Keyboard shortcuts"
          title={`Keyboard shortcuts (${formatShortcutKeys(chatShortcuts.showHelp.keys)})`}
          onclick={() => document.dispatchEvent(new CustomEvent('pontia:open-chat-shortcuts'))}
        >
          <QuestionIcon />
        </Sidebar.MenuAction>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Footer>
  <Sidebar.Rail />
</Sidebar.Root>
