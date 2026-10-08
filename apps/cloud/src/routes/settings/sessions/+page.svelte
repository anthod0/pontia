<script lang="ts">
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import { Button } from "$lib/components/ui/button";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  const browserSessions = $derived(data.sessions.filter((session) => session.kind === "browser"));
  const cliSessions = $derived(data.sessions.filter((session) => session.kind === "cli"));

  function date(value: string) {
    return new Date(value).toLocaleString();
  }
</script>

<svelte:head>
  <title>Session settings — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>Sessions</h1>
<p>Review and revoke browser sessions and CLI credentials.</p>
{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}

<section aria-labelledby="browser-heading">
  <div class="section-heading">
    <div>
      <h2 id="browser-heading">Browser sessions</h2>
      <p>Revoked sessions cannot refresh after their current access token expires.</p>
    </div>
  </div>
  {#if browserSessions.length > 0}
    <ul class="management-list">
      {#each browserSessions as session (session.id)}
        <li>
          <div>
            <h3>{session.current ? "Current browser" : "Browser session"}</h3>
            <p>
              Created {date(session.createdAt)}{session.provider ? ` · ${session.provider}` : ""}
            </p>
          </div>
          <form method="POST" action="?/revokeSession">
            <input type="hidden" name="session_id" value={session.id} />
            <Button type="submit" class="button button-secondary">Revoke</Button>
          </form>
        </li>
      {/each}
    </ul>
  {:else}
    <p>No active browser sessions.</p>
  {/if}
  {#if browserSessions.length > 1}
    <div class="batch-action">
      <AlertDialog.Root>
        <AlertDialog.Trigger class="batch-link">Sign out other browsers</AlertDialog.Trigger>
        <AlertDialog.Content>
          <AlertDialog.Header>
            <AlertDialog.Title>Sign out other browsers?</AlertDialog.Title>
            <AlertDialog.Description>
              This revokes every browser session except the one you are using now.
            </AlertDialog.Description>
          </AlertDialog.Header>
          <AlertDialog.Footer>
            <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
            <AlertDialog.Action type="submit" form="revoke-other-browsers-form">
              Sign out
            </AlertDialog.Action>
          </AlertDialog.Footer>
        </AlertDialog.Content>
      </AlertDialog.Root>
      <form id="revoke-other-browsers-form" method="POST" action="?/revokeOtherBrowsers"></form>
    </div>
  {/if}
</section>

<section aria-labelledby="cli-heading">
  <div class="section-heading">
    <div>
      <h2 id="cli-heading">CLI credentials</h2>
      <p>Credentials authorize Pontia installations to manage remote access.</p>
    </div>
  </div>
  {#if cliSessions.length > 0}
    <ul class="management-list">
      {#each cliSessions as session (session.id)}
        <li>
          <div>
            <h3>CLI credential</h3>
            <p>Created {date(session.createdAt)} · {session.id.slice(-8)}</p>
          </div>
          <form method="POST" action="?/revokeSession">
            <input type="hidden" name="session_id" value={session.id} />
            <Button type="submit" class="button button-secondary">Revoke</Button>
          </form>
        </li>
      {/each}
    </ul>
  {:else}
    <p>No active CLI credentials.</p>
  {/if}
  {#if cliSessions.length > 0}
    <div class="batch-action">
      <AlertDialog.Root>
        <AlertDialog.Trigger class="batch-link">Revoke all CLI credentials</AlertDialog.Trigger>
        <AlertDialog.Content>
          <AlertDialog.Header>
            <AlertDialog.Title>Revoke all CLI credentials?</AlertDialog.Title>
            <AlertDialog.Description>
              Every CLI installation using these credentials will need to sign in again.
            </AlertDialog.Description>
          </AlertDialog.Header>
          <AlertDialog.Footer>
            <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
            <AlertDialog.Action type="submit" form="revoke-cli-form">Revoke all</AlertDialog.Action>
          </AlertDialog.Footer>
        </AlertDialog.Content>
      </AlertDialog.Root>
      <form id="revoke-cli-form" method="POST" action="?/revokeCli"></form>
    </div>
  {/if}
</section>

<style>
  section {
    margin-top: 40px;
  }
  h2 {
    margin-bottom: 8px;
    font-size: 20px;
    letter-spacing: -0.02em;
  }
  h3 {
    margin-bottom: 4px;
    font-size: 15px;
    font-weight: 500;
  }
  section p {
    color: var(--muted-foreground);
    font-size: 14px;
    line-height: 1.7;
  }
  .section-heading {
    display: flex;
    justify-content: space-between;
    gap: 20px;
    align-items: end;
  }
  .management-list form {
    margin: 0;
  }
  .batch-action {
    display: flex;
    justify-content: flex-end;
    margin-top: 8px;
  }
  .batch-action :global(.batch-link) {
    height: auto;
    padding: 2px 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-decoration: underline;
    text-underline-offset: 3px;
    cursor: pointer;
  }
  .management-list {
    margin: 16px 0 0;
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

  @media (max-width: 600px) {
    .section-heading {
      align-items: stretch;
      flex-direction: column;
    }
    .management-list li {
      grid-template-columns: 1fr;
    }
  }
</style>
