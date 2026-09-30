<script lang="ts">
  import { dashboardPath, dashboardRelativePath } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'

  type Section = {
    label: string
    path: string
    match: string[]
  }

  const sections: Section[] = [
    { label: 'Common', path: '/settings/common', match: ['/settings/common'] },
    { label: 'Workspaces', path: '/settings/workspaces', match: ['/settings/workspaces', '/workspaces'] },
  ]

  let currentPath = $state(dashboardRelativePath())

  function isActive(section: Section): boolean {
    return section.match.some((path) => currentPath === path)
  }

  function activate(event: MouseEvent, section: Section): void {
    event.preventDefault()
    currentPath = section.path
    navigate(section.path)
  }
</script>

<svelte:window onpopstate={() => (currentPath = dashboardRelativePath())} />

<nav aria-label="Settings sections" data-settings-shell-nav="persistent" class="shrink-0 self-start md:sticky md:top-20 md:w-56">
  <div class="flex flex-col gap-1 rounded-none bg-transparent p-1">
    {#each sections as section}
      <a
        href={dashboardPath(section.path)}
        aria-current={isActive(section) ? 'page' : undefined}
        onclick={(event) => activate(event, section)}
        class="rounded-none px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground aria-[current=page]:bg-muted aria-[current=page]:text-foreground"
      >
        {section.label}
      </a>
    {/each}
  </div>
</nav>
