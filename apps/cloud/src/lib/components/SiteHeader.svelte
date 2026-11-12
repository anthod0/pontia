<script lang="ts">
  import LanguageSwitcher from "$lib/components/LanguageSwitcher.svelte";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import { m } from "$lib/paraglide/messages";
  import { localizeHref } from "$lib/paraglide/runtime";
</script>

<header class="site-header page-width">
  <a class="brand" href={localizeHref("/")} aria-label={m.home_link_label()}>
    <img src="/logo.svg" alt="" />
    <span>Pontia</span>
  </a>
  <nav aria-label={m.primary_navigation()}>
    <a class="nav-link" href={localizeHref("/docs")}>{m.nav_docs()}</a>
    <a class="nav-link" href={localizeHref("/blog")}>{m.nav_blog()}</a>
    <a class="nav-link" href={localizeHref("/settings/account")}>{m.nav_account()}</a>
    <span class="header-controls">
      <LanguageSwitcher />
      <ThemeToggle />
    </span>
  </nav>
</header>

<style>
  .header-controls {
    display: inline-flex;
    gap: 4px;
  }
</style>
