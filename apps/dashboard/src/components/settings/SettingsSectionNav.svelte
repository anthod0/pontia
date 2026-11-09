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
    { label: 'Agent Clients', path: '/settings/clients', match: ['/settings/clients'] },
    { label: 'Workspaces', path: '/settings/workspaces', match: ['/settings/workspaces', '/workspaces'] },
  ]

  let pathname = $state(window.location.pathname)
  const currentPath = $derived(dashboardRelativePath(pathname))

  function isActive(section: Section): boolean {
    return section.match.some((path) => currentPath === path)
  }

  function activate(event: MouseEvent, section: Section): void {
    event.preventDefault()
    pathname = dashboardPath(section.path)
    navigate(section.path)
  }
</script>

<svelte:window onpopstate={() => (pathname = window.location.pathname)} />

<aside class="shrink-0 self-start md:sticky md:top-20 md:w-[190px]">
  <a
    class="mb-5 block text-sm font-semibold text-heading"
    href={dashboardPath('/settings/common', pathname)}
    onclick={(event) => activate(event, sections[0])}
  >Settings</a>
  <nav aria-label="Settings sections" data-settings-shell-nav="persistent" class="flex flex-wrap gap-x-5 gap-y-2.5 border-l border-border pl-4 text-[13px] md:grid md:gap-3">
    {#each sections as section}
      <a
        href={dashboardPath(section.path, pathname)}
        aria-current={isActive(section) ? 'page' : undefined}
        onclick={(event) => activate(event, section)}
        class="text-muted-foreground transition-colors hover:text-primary aria-[current=page]:text-primary"
      >
        {section.label}
      </a>
    {/each}
  </nav>
</aside>
