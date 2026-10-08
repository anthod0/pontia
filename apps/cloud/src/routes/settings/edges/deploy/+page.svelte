<script lang="ts">
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import CopyIcon from "phosphor-svelte/lib/CopyIcon";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { deploymentModeCommand } from "$lib/edge-deployment-command";
  import type { ActionData } from "./$types";

  let { form }: { form: ActionData } = $props();
  let mode = $state<"default" | "custom">("default");
  let port = $state<number | undefined>(8443);
  let copiedCommand = $state<string | null>(null);
  let command = $derived(
    form?.deployment ? deploymentModeCommand(form.deployment.command, mode, port) : null,
  );

  async function copyCommand() {
    if (!command) return;
    const copied = command;
    await navigator.clipboard.writeText(copied);
    copiedCommand = copied;
  }
</script>

<svelte:head>
  <title>Edge deployment command — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>Deploy a self-hosted edge</h1>

  {#if form?.error}
    <p class="auth-error" role="alert">{form.error}</p>
  {/if}

  {#if form?.deployment}
    <section aria-labelledby="deployment-name">
      <h2 id="deployment-name">{form.deployment.name}</h2>
      <p>Run this command on your public Linux server.</p>
      <p>This command expires at {new Date(form.deployment.expiresAt).toLocaleString()}.</p>
      <div id="custom-port-options">
        {#if mode === "custom"}
          <h3 id="custom-port-label">Custom port</h3>
          <div class="port-field">
            <Input
              id="edge-port"
              aria-labelledby="custom-port-label"
              type="number"
              min={1}
              max={65535}
              step={1}
              bind:value={port}
              aria-invalid={command === null}
            />
          </div>
        {/if}
      </div>

      {#if command}
        <pre><code>{command}</code></pre>
      {:else}
        <p class="auth-error" role="alert">
          Enter an integer from 1 to 65535 that is not blocked by browsers or the control plane.
          Recommended: 8443.
        </p>
      {/if}
      <Button type="button" disabled={!command} onclick={copyCommand}>
        {#if command && copiedCommand === command}<CheckIcon />{:else}<CopyIcon />{/if}
        {command && copiedCommand === command ? "Copied" : "Copy command"}
      </Button>
      <div class="alternative-action">
        <Button
          type="button"
          variant="link"
          class="alternative-toggle"
          aria-expanded={mode === "custom"}
          aria-controls="custom-port-options"
          onclick={() => (mode = mode === "default" ? "custom" : "default")}
        >
          {mode === "custom"
            ? "Restore default deployment"
            : "Can't use port 80 or 443? Use an alternative deployment"}
        </Button>
      </div>
    </section>
  {:else if !form?.error}
    <p>Generate a deployment command from the edges page to get started.</p>
  {/if}

<p class="back-link"><a href="/settings/edges">Back to edges</a></p>

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
  }

  h3 {
    font-size: 14px;
    font-weight: 500;
    margin-top: 20px;
  }

  .alternative-action {
    margin-top: 12px;
  }

  .alternative-action :global(.alternative-toggle) {
    width: auto;
    max-width: 100%;
    height: auto;
    padding: 0;
    color: var(--muted-foreground);
    white-space: normal;
    text-align: left;
  }

  .port-field {
    display: grid;
    gap: 8px;
    max-width: 240px;
  }

  pre {
    background: var(--muted);
    border: 1px solid var(--border);
    border-radius: 6px;
    margin-block: 16px;
    overflow-x: auto;
    padding: 16px;
    white-space: pre;
  }

  code {
    overflow-wrap: anywhere;
  }
</style>
