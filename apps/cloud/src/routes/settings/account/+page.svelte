<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { authError } from "$lib/auth-errors";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  const message = $derived(authError(data.error) ?? form?.error);
  const providers = [
    { id: "google", name: "Google" },
    { id: "github", name: "GitHub" },
  ] as const;
</script>

<svelte:head>
  <title>Account settings — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>Account</h1>
<p>Manage your profile and the accounts you use to sign in to Pontia.</p>
{#if message}<p class="auth-error" role="alert">{message}</p>{/if}

<section aria-labelledby="profile-heading">
  <h2 id="profile-heading">Profile</h2>
  <div class="profile">
    {#if data.profile.avatarUrl}
      <img src={data.profile.avatarUrl} alt="" referrerpolicy="no-referrer" />
    {/if}
    <h3>{data.profile.displayName ?? "Pontia user"}</h3>
  </div>
</section>

<section aria-labelledby="methods-heading">
  <h2 id="methods-heading">Sign-in methods</h2>
  <ul class="management-list">
    {#each providers as provider}
      {@const account = data.accounts.find((item) => item.provider === provider.id)}
      <li>
        <div>
          <h3>{provider.name}</h3>
          {#if account}
            <p>
              Linked{account.email ? ` as ${account.email}` : ""}
              {account.email
                ? account.emailVerified
                  ? " · Verified email"
                  : " · Unverified email"
                : ""}
            </p>
          {:else}
            <p>Not linked</p>
          {/if}
        </div>
        {#if account}
          <form method="POST" action="?/unlinkProvider">
            <input type="hidden" name="provider" value={provider.id} />
            <Button type="submit" class="button button-secondary">Unlink</Button>
          </form>
        {:else}
          <form method="POST" action={`/api/auth/${provider.id}/bind`}>
            <Button type="submit" class="button button-secondary">Link</Button>
          </form>
        {/if}
      </li>
    {/each}
  </ul>
</section>

<form method="POST" action="/api/auth/logout" class="sign-out-form">
  <Button type="submit" class="button">Sign out</Button>
</form>

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
  .profile {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 16px;
    align-items: center;
    margin-top: 16px;
  }
  .profile img {
    width: 56px;
    height: 56px;
    border-radius: 50%;
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
  .management-list form {
    margin: 0;
  }
  .sign-out-form {
    max-width: 220px;
    margin-top: 40px;
  }
  .sign-out-form :global(.button) {
    width: 100%;
  }

  @media (max-width: 600px) {
    .management-list li {
      grid-template-columns: 1fr;
    }
  }
</style>
