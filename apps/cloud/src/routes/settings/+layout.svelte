<script lang="ts">
  import { page } from "$app/state";
  import SiteHeader from "$lib/components/SiteHeader.svelte";
  import type { Snippet } from "svelte";
  import { m } from "$lib/paraglide/messages";
  import { deLocalizeHref, localizeHref } from "$lib/paraglide/runtime";
  let { children }: { children: Snippet } = $props();
  const sections = [
    { href: "/settings/account", label: m.settings_account },
    { href: "/settings/sessions", label: m.settings_sessions },
    { href: "/settings/devices", label: m.settings_devices },
    { href: "/settings/edges", label: m.settings_edges },
  ];
  const pathname = $derived(deLocalizeHref(page.url.pathname));

  function active(href: string) {
    return pathname === href || pathname.startsWith(`${href}/`);
  }
</script>

<a class="skip-link" href="#settings-content">{m.skip_to_settings()}</a>
<SiteHeader />

<div class="settings-shell page-width">
  <aside>
    <a class="settings-title" href={localizeHref("/settings/account")}>{m.settings_heading()}</a>
    <nav aria-label={m.settings_sections()}>
      {#each sections as section}
        <a href={localizeHref(section.href)} aria-current={active(section.href) ? "page" : undefined}>
          {section.label()}
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
    font-size: var(--text-28);
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.025em;
  }

  .settings-content :global(h2) {
    font-size: 18px;
    font-weight: 500;
    margin-bottom: 8px;
  }
  .settings-content :global(h3) {
    font-size: var(--text-15);
    font-weight: 500;
    margin-bottom: 4px;
  }
  .settings-content :global(:where(p)) {
    color: var(--muted-foreground);
    font-size: 14px;
    line-height: 1.6;
  }
  .settings-content :global(.metadata) {
    font: 12px/1.6 var(--font-mono);
  }
  .settings-content :global(> p) {
    margin-block: 14px;
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

</style>
