<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { ParaglideMessage } from "@inlang/paraglide-js-svelte";
  import { addCodeCopyButtons } from "$lib/code-copy";
  import "$lib/styles/article.css";
  import { m } from "$lib/paraglide/messages";
  import { getLocale, localizeHref } from "$lib/paraglide/runtime";
  import type { ActionData } from "./$types";

  let { form }: { form: ActionData } = $props();
</script>

<svelte:head>
  <title>{m.deploy_title()}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>{m.edges_deploy_heading()}</h1>

{#if form?.deployment}
  <section aria-labelledby="deployment-hostname">
    <h2 id="deployment-hostname">{form.deployment.hostname}</h2>
    <p>{m.deploy_run()}</p>
    <p>{m.deploy_expires({ time: new Date(form.deployment.expiresAt).toLocaleString(getLocale()) })}</p>
    <div class="article-content" {@attach addCodeCopyButtons}>
      <pre><code>{form.deployment.command}</code></pre>
    </div>

    <div class="retry-help">
      <h3>{m.deploy_retry_heading()}</h3>
      <p>{m.deploy_retry_text()}</p>
      <div class="article-content" {@attach addCodeCopyButtons}>
        <pre><code>sudo pontia-edge retry</code></pre>
      </div>
    </div>
    <p class="another-link"><a href={localizeHref("/settings/edges/deploy")}>{m.deploy_another()}</a></p>
  </section>
{:else}
  <p class="intro">{m.edges_deploy_text()}</p>
  <ul class="requirements">
    <li>{m.deploy_dns_requirement()}</li>
    <li>{m.deploy_port_requirement()}</li>
  </ul>

  <form class="deployment-form" method="POST">
    <label for="edge-hostname">{m.deploy_hostname_label()}</label>
    <div class="hostname-field">
      <Input
        id="edge-hostname"
        name="hostname"
        type="text"
        inputmode="url"
        autocomplete="off"
        autocapitalize="none"
        spellcheck={false}
        placeholder="edge.example.com"
        maxlength={253}
        value={form && "hostname" in form ? form.hostname : ""}
        aria-invalid={form?.error ? true : undefined}
        aria-describedby={form?.error ? "deployment-error" : undefined}
        required
      />
    </div>
    {#if form?.error}
      <p id="deployment-error" class="auth-error" role="alert">{form.error}</p>
    {/if}
    <Button type="submit" class="button">{m.edges_deploy_generate()}</Button>
    <p class="agreement">
      <ParaglideMessage message={m.edges_deploy_agreement}>
        {#snippet agreement({ children })}<a href="https://letsencrypt.org/repository/" target="_blank" rel="noreferrer">{@render children?.()}</a>{/snippet}
      </ParaglideMessage>
    </p>
  </form>
{/if}

<p class="back-link"><a href={localizeHref("/settings/edges")}>{m.deploy_back()}</a></p>

<style>
  section {
    margin-block: 28px;
  }

  .back-link {
    margin-top: 28px;
  }

  h2 {
    font-size: 18px;
    margin-bottom: 8px;
    overflow-wrap: anywhere;
  }

  h3 {
    font-size: 14px;
    font-weight: 500;
    margin-top: 20px;
  }

  .intro,
  .requirements,
  .agreement,
  .retry-help p {
    color: var(--muted-foreground);
    font-size: 14px;
    line-height: 1.7;
  }

  .requirements {
    margin-top: 8px;
    padding-left: 20px;
    list-style: disc;
  }

  .deployment-form {
    display: flex;
    align-items: flex-start;
    flex-direction: column;
    gap: 12px;
    margin-top: 24px;
  }

  .deployment-form label {
    font-size: 14px;
    font-weight: 500;
  }

  .hostname-field {
    width: 100%;
    max-width: 420px;
  }

  .agreement {
    max-width: 620px;
    margin: 0;
  }

  .agreement a {
    color: var(--primary);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .retry-help {
    margin-top: 28px;
  }

  .another-link {
    margin-top: 20px;
    font-size: 14px;
  }
</style>
