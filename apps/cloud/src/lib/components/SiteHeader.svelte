<script lang="ts">
  let { logo = true }: { logo?: boolean } = $props();
</script>

<header class="site-header page-width">
  <a class="brand" href="/" aria-label="Pontia home">
    {#if logo}<img src="/logo.svg" alt="" />{/if}
    <span>Pontia</span>
  </a>
  <nav aria-label="Primary navigation">
    <a class="nav-link" href="/docs">Docs</a>
    <a class="nav-link" href="/settings/account">Account</a>
  </nav>
</header>
