<script lang="ts">
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import { Button } from "$lib/components/ui/button";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  const browserSessions = $derived(data.sessions.filter((session) => session.kind === "browser"));

  function date(value: string) {
    return new Date(value).toLocaleString();
  }
</script>

<svelte:head>
  <title>Session settings — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>Sessions</h1>
<p>Review and revoke browser sessions.</p>
{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}

<section>
  {#if browserSessions.length > 0}
    <ul class="management-list">
      {#each browserSessions as session (session.id)}
        <li>
          <div>
            <h3>{session.current ? "Current browser" : "Browser session"}</h3>
            <p>
              Last login {session.lastSeenAt ? date(session.lastSeenAt) : "unavailable"}{session.country ? ` · ${session.country}` : ""}{session.provider ? ` · ${session.provider}` : ""}
            </p>
          </div>
          <form method="POST" action="?/revokeSession">
            <input type="hidden" name="session_id" value={session.id} />
            <Button
              type="submit"
              variant="destructive"
              size="icon"
              aria-label={session.current ? "Revoke current browser session" : "Revoke browser session"}
              title="Revoke session"
            >
              <TrashIcon size={16} />
            </Button>
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

<style>
  section {
    margin-top: 40px;
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
    .management-list li {
      grid-template-columns: 1fr;
    }
  }
</style>
