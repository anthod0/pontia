<script lang="ts">
  import ArrowUpRightIcon from "phosphor-svelte/lib/ArrowUpRightIcon";

  const repository = "https://github.com/anthod0/pontia";
</script>

<footer class="site-footer page-width">
  <a class="brand footer-brand" href="/" aria-label="Pontia home">
    <img src="/logo.svg" alt="" />
    <span>Pontia</span>
  </a>
  <p>Your device. Your agent cloud.</p>
  <nav aria-label="Footer navigation">
    <a href={repository}>GitHub<ArrowUpRightIcon size={12} /></a>
    <a href="/docs">Documentation</a>
    <a href="/privacy">Privacy</a>
    <a href="/terms">Terms</a>
    <a href={`${repository}/blob/main/LICENSE`}>Apache 2.0</a>
  </nav>
</footer>
