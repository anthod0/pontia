<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();

  function date(value: string) {
    return new Date(value).toLocaleString();
  }
</script>

<svelte:head>
  <title>Device settings — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>Devices</h1>
<p>Manage devices registered for remote Dashboard access.</p>
{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}

{#if data.devices.length > 0}
  <ul class="management-list">
    {#each data.devices as device (device.id)}
      <li>
        <div>
          <h2>{device.name}</h2>
          <p>{device.edgeName} · Registered {date(device.createdAt)}</p>
        </div>
        <form method="POST" action="?/removeDevice">
          <input type="hidden" name="device_id" value={device.id} />
          <Button type="submit" class="button button-secondary">Remove remote access</Button>
        </form>
      </li>
    {/each}
  </ul>
{:else}
  <p class="empty">No registered devices.</p>
{/if}

<style>
  .empty {
    margin-top: 28px;
    color: var(--muted-foreground);
    font-size: 14px;
    line-height: 1.7;
  }
  .management-list {
    margin: 28px 0 0;
    padding: 0;
    list-style: none;
  }
  .management-list li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 20px;
    align-items: center;
    border-top: 1px solid var(--border);
    padding-block: 16px;
  }
  .management-list li:last-child {
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin-bottom: 4px;
    font-size: 15px;
    font-weight: 500;
  }
  .management-list p {
    color: var(--muted-foreground);
    font-size: 14px;
    line-height: 1.7;
  }
  form {
    margin: 0;
  }

  @media (max-width: 600px) {
    .management-list li {
      grid-template-columns: 1fr;
    }
  }
</style>
