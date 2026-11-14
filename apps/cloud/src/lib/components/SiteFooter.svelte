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
    <a href={repository}>GitHub<ArrowUpRightIcon size={12} /></a>
    <a href={localizeHref("/docs")}>{m.footer_documentation()}</a>
    <a href="/llms.txt" target="_blank">llms.txt</a>
    <a href={localizeHref("/blog")}>{m.nav_blog()}</a>
    <a href="/privacy">{m.footer_privacy()}</a>
    <a href="/terms">{m.footer_terms()}</a>
    <a href={`${repository}/blob/main/LICENSE`}>Apache 2.0</a>
  </nav>
</footer>
