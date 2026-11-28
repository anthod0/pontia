<script lang="ts">
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import { buttonVariants } from "$lib/components/ui/button";
  import { m } from "$lib/paraglide/messages";
  import { getLocale } from "$lib/paraglide/runtime";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  const hasOtherBrowser = $derived(data.sessions.some((session) => session.kind === "browser" && !session.current));

  function date(value: string) {
    return new Date(value).toLocaleString(getLocale());
  }
</script>

<svelte:head>
  <title>{m.sessions_title()}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>{m.settings_sessions()}</h1>
<p>{m.sessions_intro()}</p>
{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}

<section>
  {#if data.sessions.length > 0}
    <ul class="management-list">
      {#each data.sessions as session (session.id)}
        {@const revokeLabel = session.kind === "cli" ? m.sessions_revoke_cli() : session.current ? m.sessions_revoke_current() : m.sessions_revoke_browser()}
        <li>
          <div>
            <h3>{session.kind === "cli" ? m.sessions_cli() : session.current ? m.sessions_current() : m.sessions_browser()}</h3>
            <p>
              {session.lastSeenAt ? m.sessions_last_active({ date: date(session.lastSeenAt) }) : m.sessions_last_active_unavailable()}{session.country ? ` · ${session.country}` : ""}{session.provider ? ` · ${session.provider}` : ""}
            </p>
          </div>
          <form id={`revoke-session-${session.id}`} method="POST" action="?/revokeSession">
            <input type="hidden" name="session_id" value={session.id} />
            <AlertDialog.Root>
              <AlertDialog.Trigger
                type="button"
                class={buttonVariants({ variant: "destructive", size: "icon" })}
                aria-label={revokeLabel}
                title={revokeLabel}
              >
                <TrashIcon size={16} />
              </AlertDialog.Trigger>
              <AlertDialog.Content>
                <AlertDialog.Header>
                  <AlertDialog.Title>{revokeLabel}</AlertDialog.Title>
                  <AlertDialog.Description>
                    {session.kind === "cli" ? m.sessions_revoke_cli_text() : m.sessions_revoke_browser_text()}
                  </AlertDialog.Description>
                </AlertDialog.Header>
                <AlertDialog.Footer>
                  <AlertDialog.Cancel>{m.cancel()}</AlertDialog.Cancel>
                  <AlertDialog.Action variant="destructive" type="submit" form={`revoke-session-${session.id}`}>
                    {m.sessions_revoke_confirm()}
                  </AlertDialog.Action>
                </AlertDialog.Footer>
              </AlertDialog.Content>
            </AlertDialog.Root>
          </form>
        </li>
      {/each}
    </ul>
  {:else}
    <p>{m.sessions_none()}</p>
  {/if}
  {#if hasOtherBrowser}
    <div class="batch-action">
      <AlertDialog.Root>
        <AlertDialog.Trigger class="batch-link">{m.sessions_sign_out_others()}</AlertDialog.Trigger>
        <AlertDialog.Content>
          <AlertDialog.Header>
            <AlertDialog.Title>{m.sessions_sign_out_others_title()}</AlertDialog.Title>
            <AlertDialog.Description>
              {m.sessions_sign_out_others_text()}
            </AlertDialog.Description>
          </AlertDialog.Header>
          <AlertDialog.Footer>
            <AlertDialog.Cancel>{m.cancel()}</AlertDialog.Cancel>
            <AlertDialog.Action variant="destructive" type="submit" form="revoke-other-browsers-form">
              {m.sign_out()}
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
