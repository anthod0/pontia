<script lang="ts">
  import ManagementList from "$lib/components/ManagementList.svelte";
  import { Alert } from "$lib/components/ui/alert";
  import { addCodeCopyButtons } from "$lib/code-copy";
  import "$lib/styles/article.css";
  import { Button } from "$lib/components/ui/button";
  import { authError } from "$lib/auth-errors";
  import { m } from "$lib/paraglide/messages";
  import { getLocale, localizeHref } from "$lib/paraglide/runtime";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  const message = $derived(authError(data.error) ?? form?.error);
  const installCommand = 'npx @pontia/cli init';
  const providers = [
    { id: "google", name: "Google" },
    { id: "github", name: "GitHub" },
  ] as const;
</script>

<svelte:head>
  <title>{m.account_title()}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>{m.settings_account()}</h1>
<p>{m.account_intro()}</p>
{#if message}<Alert variant="destructive" class="my-4">{message}</Alert>{/if}

{#if !data.hasDevices}
  <section class="getting-started" aria-labelledby="getting-started-heading">
    <h2 id="getting-started-heading">{m.account_first_device_heading()}</h2>
    <p>{m.account_first_device_text()}</p>
    <div class="article-content" {@attach addCodeCopyButtons}>
      <pre><code>{installCommand}</code></pre>
    </div>
    <Button href={localizeHref("/docs/getting-started")} size="lg">{m.docs_getting_started()}</Button>
  </section>
{/if}

<section aria-labelledby="profile-heading">
  <h2 id="profile-heading">{m.account_profile()}</h2>
  <div class="profile">
    {#if data.profile.avatarUrl}
      <img src={data.profile.avatarUrl} alt="" referrerpolicy="no-referrer" />
    {/if}
    <h3>{data.profile.displayName ?? m.account_default_name()}</h3>
  </div>
</section>

<section aria-labelledby="methods-heading">
  <h2 id="methods-heading">{m.account_sign_in_methods()}</h2>
  <ManagementList>
    {#each providers as provider}
      {@const account = data.accounts.find((item) => item.provider === provider.id)}
      <li>
        <div>
          <h3>{provider.name}</h3>
          {#if account}
            <p>
              {account.email
                ? account.emailVerified
                  ? m.account_linked_verified({ email: account.email })
                  : m.account_linked_unverified({ email: account.email })
                : m.account_linked()}
            </p>
          {:else}
            <p>{m.account_not_linked()}</p>
          {/if}
        </div>
        {#if account}
          <form method="POST" action="?/unlinkProvider">
            <input type="hidden" name="provider" value={provider.id} />
            <Button type="submit" size="lg" variant="outline">{m.account_unlink()}</Button>
          </form>
        {:else}
          <form method="POST" action={`/api/auth/${provider.id}/bind?locale=${getLocale()}`}>
            <Button type="submit" size="lg" variant="outline">{m.account_link()}</Button>
          </form>
        {/if}
      </li>
    {/each}
  </ManagementList>
</section>

<form method="POST" action={`/api/auth/logout?locale=${getLocale()}`} class="sign-out-form">
  <Button type="submit" size="lg" class="w-full">{m.sign_out()}</Button>
</form>

<style>
  section {
    margin-top: 40px;
  }
  .getting-started {
    border-block: 1px solid var(--border);
    padding-block: 24px;
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
  .sign-out-form {
    max-width: 220px;
    margin-top: 40px;
  }
</style>
