<script lang="ts">
  import ArrowSquareOutIcon from "phosphor-svelte/lib/ArrowSquareOutIcon";
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { m } from "$lib/paraglide/messages";
  import { getLocale } from "$lib/paraglide/runtime";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  const deviceHandlePattern = "[a-z][a-z0-9_-]{3,47}";

  function date(value: string) {
    return new Date(value).toLocaleString(getLocale());
  }
</script>

<svelte:head>
  <title>{m.devices_title()}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>{m.settings_devices()}</h1>
<p>{m.devices_intro()}</p>
{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}

{#if data.devices.length > 0}
  <ul class="management-list">
    {#each data.devices as device (device.id)}
      <li>
        <div class="device-details">
          <div class="device-heading">
            <h2>{device.name}</h2>
            <a
              class="device-handle"
              href={`https://app.pontia.dev/${device.handle}`}
              target="_blank"
              rel="noreferrer"
              title={m.devices_open({ name: device.name })}
            >
              <code>{device.handle}</code>
              <ArrowSquareOutIcon size={13} />
            </a>
          </div>
          <p>{m.devices_registered({ edge: device.edgeName, date: date(device.createdAt) })}</p>
        </div>
        <div class="device-actions">
          <Dialog.Root>
            <Dialog.Trigger class="row-action" aria-label={m.devices_edit_named({ name: device.name })} title={m.devices_edit()}>
              <PencilSimpleIcon size={16} />
            </Dialog.Trigger>
            <Dialog.Content>
              <Dialog.Header>
                <Dialog.Title>{m.devices_edit_named({ name: device.name })}</Dialog.Title>
                <Dialog.Description>
                  {m.devices_edit_text()}
                </Dialog.Description>
              </Dialog.Header>
              <form class="edit-dialog-form" method="POST" action="?/updateDevice">
                <input type="hidden" name="device_id" value={device.id} />
                <label>
                  <span>{m.devices_name()}</span>
                  <Input name="name" value={device.name} required />
                </label>
                <label>
                  <span>{m.devices_handle()}</span>
                  <Input
                    name="handle"
                    value={device.handle}
                    minlength={4}
                    maxlength={48}
                    pattern={deviceHandlePattern}
                    aria-describedby={`handle-help-${device.id}`}
                    required
                  />
                </label>
                <p id={`handle-help-${device.id}`} class="field-help">
                  {m.devices_handle_help()}
                </p>
                <Dialog.Footer>
                  <Dialog.Close>
                    {#snippet child({ props })}
                      <Button variant="outline" {...props}>{m.cancel()}</Button>
                    {/snippet}
                  </Dialog.Close>
                  <Button type="submit">{m.save()}</Button>
                </Dialog.Footer>
              </form>
            </Dialog.Content>
          </Dialog.Root>
          <form method="POST" action="?/removeDevice">
            <input type="hidden" name="device_id" value={device.id} />
            <Button
              type="submit"
              variant="destructive"
              size="icon"
              aria-label={m.devices_remove_named({ name: device.name })}
              title={m.devices_remove()}
            >
              <TrashIcon size={16} />
            </Button>
          </form>
        </div>
      </li>
    {/each}
  </ul>
{:else}
  <p class="empty">{m.devices_none()}</p>
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
  .device-heading {
    display: flex;
    min-width: 0;
    align-items: baseline;
    gap: 16px;
  }
  .device-handle {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    gap: 3px;
    overflow: hidden;
    color: var(--muted-foreground);
    text-decoration: none;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .device-handle:hover,
  .device-handle:focus-visible {
    color: var(--foreground);
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  code {
    font-size: 13px;
  }
  form {
    margin: 0;
  }
  .device-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }
  .device-actions :global(.row-action) {
    display: inline-flex;
    width: 32px;
    height: 32px;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--background);
    color: var(--muted-foreground);
    cursor: pointer;
  }
  .device-actions :global(.row-action:hover),
  .device-actions :global(.row-action:focus-visible) {
    background: var(--muted);
    color: var(--foreground);
  }
  .edit-dialog-form {
    display: grid;
    gap: 16px;
  }
  .edit-dialog-form label {
    display: grid;
    gap: 6px;
    font-size: 13px;
    font-weight: 500;
  }
  .field-help {
    margin-top: -8px;
  }

  @media (max-width: 760px) {
    .management-list li {
      grid-template-columns: 1fr;
    }
    .device-actions {
      flex-wrap: wrap;
      justify-content: flex-start;
    }
  }
</style>
