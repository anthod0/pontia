<script lang="ts">
  import { page } from "$app/state";
  import SiteHeader from "$lib/components/SiteHeader.svelte";
  import type { Snippet } from "svelte";
  let { children }: { children: Snippet } = $props();
  const sections = [
    { href: "/settings/account", label: "Account" },
    { href: "/settings/sessions", label: "Sessions" },
    { href: "/settings/devices", label: "Devices" },
    { href: "/settings/edges", label: "Edges" },
  ];

  function active(href: string) {
    return page.url.pathname === href || page.url.pathname.startsWith(`${href}/`);
  }
</script>

<a class="skip-link" href="#settings-content">Skip to settings</a>
<SiteHeader />

<div class="settings-shell page-width">
  <aside>
    <a class="settings-title" href="/settings/account">Settings</a>
    <nav aria-label="Settings sections">
      {#each sections as section}
        <a href={section.href} aria-current={active(section.href) ? "page" : undefined}>
          {section.label}
        </a>
      {/each}
    </nav>
  </aside>
  <main id="settings-content" class="settings-content">{@render children()}</main>
</div>

<style>
  .settings-shell {
    display: grid;
    grid-template-columns: 190px minmax(0, 720px);
    gap: 72px;
    padding-block: 64px 96px;
  }
  aside {
    position: sticky;
    top: 32px;
    align-self: start;
  }
  .settings-title {
    display: block;
    margin-bottom: 20px;
    color: var(--heading);
    font-size: 14px;
    font-weight: 600;
  }
  aside nav {
    display: grid;
    gap: 12px;
    border-left: 1px solid var(--border);
    padding-left: 16px;
    color: var(--muted-foreground);
    font-size: 13px;
  }
  aside nav a:hover,
  aside nav a[aria-current="page"] {
    color: var(--primary);
  }
  .settings-content {
    min-width: 0;
  }
  .settings-content :global(h1) {
    margin-bottom: 18px;
    color: var(--heading);
    font-size: clamp(36px, 5vw, 52px);
    font-weight: 500;
    line-height: 1.1;
    letter-spacing: -0.045em;
  }
  .settings-content :global(> p) {
    margin-block: 14px;
    color: var(--muted-foreground);
    font-size: 15px;
    line-height: 1.8;
  }
  .settings-content :global(.auth-error) {
    border: 1px solid #dab0aa;
    margin-block: 20px;
    padding: 12px;
    color: #9d3229;
  }

  @media (max-width: 760px) {
    .settings-shell {
      grid-template-columns: 1fr;
      gap: 36px;
      padding-block: 36px 64px;
    }
    aside {
      position: static;
    }
    aside nav {
      display: flex;
      flex-wrap: wrap;
      gap: 10px 20px;
    }
  }

  @media (max-width: 600px) {
    .settings-content :global(> p) {
      font-size: 14px;
    }
  }
</style>
