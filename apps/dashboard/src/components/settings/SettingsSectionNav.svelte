<script lang="ts">
  import { dashboardPath, dashboardRelativePath } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'
  import { isSettingsSectionActive, settingsSections as sections, type SettingsSection as Section } from './sections'

  let pathname = $state(window.location.pathname)
  const currentPath = $derived(dashboardRelativePath(pathname))

  function isActive(section: Section): boolean {
    return isSettingsSectionActive(section, currentPath)
  }

  function activate(event: MouseEvent, section: Section): void {
    event.preventDefault()
    pathname = dashboardPath(section.path)
    navigate(section.path)
  }
</script>

<svelte:window onpopstate={() => (pathname = window.location.pathname)} />

<nav aria-label="Settings sections" class="flex min-w-0 gap-1 overflow-x-auto text-sm">
  {#each sections as section}
    <a
      href={dashboardPath(section.path, pathname)}
      aria-current={isActive(section) ? 'page' : undefined}
      onclick={(event) => activate(event, section)}
      class="shrink-0 rounded-md px-3 py-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-heading aria-[current=page]:bg-muted aria-[current=page]:font-medium aria-[current=page]:text-heading"
    >
      {section.label}
    </a>
  {/each}
</nav>
