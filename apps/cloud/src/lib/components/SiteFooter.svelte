<script lang="ts">
  import ArrowUpRightIcon from "phosphor-svelte/lib/ArrowUpRightIcon";
  import { m } from "$lib/paraglide/messages";
  import { localizeHref } from "$lib/paraglide/runtime";

  const repository = "https://github.com/anthod0/pontia";
</script>

<footer class="site-footer page-width">
  <a class="brand footer-brand" href={localizeHref("/")} aria-label={m.home_link_label()}>
    <img src="/logo.svg" alt="" />
    <span>Pontia</span>
  </a>
  <p>Your device. Your agent cloud.</p>
  <nav aria-label={m.footer_navigation()}>
    <a href={repository} target="_blank" rel="noopener">GitHub<ArrowUpRightIcon size={12} /></a>
    <a href="https://x.com/anthod0" target="_blank" rel="noopener">X<ArrowUpRightIcon size={12} /></a>
    <a href="/llms.txt" target="_blank">llms.txt<ArrowUpRightIcon size={12} /></a>
    <a href="/privacy">{m.footer_privacy()}</a>
    <a href="/terms">{m.footer_terms()}</a>
  </nav>
</footer>
