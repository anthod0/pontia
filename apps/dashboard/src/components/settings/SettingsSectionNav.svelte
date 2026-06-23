<script lang="ts">
  import { navigate } from 'svelte-mini-router'

  type Section = {
    label: string
    href: string
    path: string
    match: string[]
  }

  const sections: Section[] = [
    { label: 'Common', href: '/dashboard/settings/common', path: '/settings/common', match: ['/dashboard/settings/common'] },
    { label: 'Workspaces', href: '/dashboard/settings/workspaces', path: '/settings/workspaces', match: ['/dashboard/settings/workspaces', '/dashboard/workspaces'] },
  ]

  let currentPath = $state(window.location.pathname)

  function isActive(section: Section): boolean {
    return section.match.some((path) => currentPath === path)
  }

  function activate(event: MouseEvent, section: Section): void {
    event.preventDefault()
    currentPath = new URL(section.href, window.location.origin).pathname
    navigate(section.path)
  }
</script>

<svelte:window onpopstate={() => (currentPath = window.location.pathname)} />

<nav aria-label="Settings sections" data-settings-shell-nav="persistent" class="shrink-0 self-start md:sticky md:top-20 md:w-56">
  <div class="flex flex-col gap-1 rounded-lg bg-transparent p-1">
    {#each sections as section}
      <a
        href={section.href}
        aria-current={isActive(section) ? 'page' : undefined}
        onclick={(event) => activate(event, section)}
        class="rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground aria-[current=page]:bg-muted aria-[current=page]:text-foreground"
      >
        {section.label}
      </a>
    {/each}
  </div>
</nav>
