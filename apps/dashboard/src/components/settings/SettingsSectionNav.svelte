<script lang="ts">
  type Section = {
    label: string
    href: string
    match: string[]
  }

  const sections: Section[] = [
    { label: 'Common', href: '/dashboard/settings/common', match: ['/dashboard/settings/common'] },
    { label: 'Workspaces', href: '/dashboard/settings/workspaces', match: ['/dashboard/settings/workspaces', '/dashboard/workspaces'] },
    { label: 'Agent Profiles', href: '/dashboard/settings/agent-profiles', match: ['/dashboard/settings/agent-profiles', '/dashboard/agent-profiles'] },
  ]

  let currentPath = $state(window.location.pathname)

  function isActive(section: Section): boolean {
    return section.match.some((path) => currentPath === path)
  }

  function activate(path: string): void {
    currentPath = new URL(path, window.location.origin).pathname
  }
</script>

<svelte:window onpopstate={() => (currentPath = window.location.pathname)} />

<nav aria-label="Settings sections" data-settings-shell-nav="persistent" class="shrink-0 md:sticky md:top-20 md:w-56">
  <div class="flex flex-col gap-1 rounded-lg border bg-card p-1">
    {#each sections as section}
      <a
        href={section.href}
        aria-current={isActive(section) ? 'page' : undefined}
        onclick={() => activate(section.href)}
        class="rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground aria-[current=page]:bg-primary aria-[current=page]:text-primary-foreground"
      >
        {section.label}
      </a>
    {/each}
  </div>
</nav>
