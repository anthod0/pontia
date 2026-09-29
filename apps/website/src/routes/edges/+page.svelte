<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import AuthLayout from "$lib/components/AuthLayout.svelte";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  let copied = $state(false);

  async function copyCommand(command: string) {
    await navigator.clipboard.writeText(command);
    copied = true;
  }
</script>

<svelte:head>
  <title>Self-hosted edges — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<AuthLayout>
  <h1>Deploy a self-hosted edge</h1>
  <p>Generate a one-time command, then run it on your public Linux server.</p>

  {#if form?.error}
    <p class="auth-error" role="alert">{form.error}</p>
  {/if}

  {#if form?.deployment}
    <section aria-labelledby="deployment-name">
      <h2 id="deployment-name">{form.deployment.name}</h2>
      <p>This command expires at {new Date(form.deployment.expiresAt).toLocaleString()}.</p>
      <pre><code>{form.deployment.command}</code></pre>
      <Button type="button" onclick={() => copyCommand(form.deployment!.command)}>
        {copied ? "Copied" : "Copy command"}
      </Button>
    </section>
  {:else}
    <form method="POST">
      <Button type="submit">Generate deployment command</Button>
    </form>
  {/if}

  <p><a href="/account">Back to account</a></p>
</AuthLayout>

<style>
  section {
    margin-block: 28px;
  }

  h2 {
    font-size: 18px;
    margin-bottom: 8px;
  }

  pre {
    background: var(--muted);
    border: 1px solid var(--border);
    border-radius: 6px;
    margin-block: 16px;
    overflow-x: auto;
    padding: 16px;
    white-space: pre-wrap;
  }

  code {
    overflow-wrap: anywhere;
  }
</style>
