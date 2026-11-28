<script lang="ts">
  import ManagementList from "$lib/components/ManagementList.svelte";
  import { Alert } from "$lib/components/ui/alert";
  import ArrowSquareOutIcon from "phosphor-svelte/lib/ArrowSquareOutIcon";
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import { Button, buttonVariants } from "$lib/components/ui/button";
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
{#if form?.error}<Alert variant="destructive" class="my-4">{form.error}</Alert>{/if}

{#if data.devices.length > 0}
  <ManagementList>
    {#each data.devices as device (device.id)}
      <li>
        <div class="device-details">
          <div class="device-heading">
            <h3>{device.name}</h3>
            <a
              class="device-handle"
              href={`https://app.pontia.dev/${device.handle}`}
              target="_blank"
              rel="noreferrer"
              title={m.devices_open({ name: device.name })}
            >
              <code class="inline-code">{device.handle}</code>
              <ArrowSquareOutIcon size={13} />
            </a>
          </div>
          <p class="metadata">{m.devices_registered({ edge: device.edgeName, date: date(device.createdAt) })}</p>
        </div>
        <div class="device-actions">
          <Dialog.Root>
            <Dialog.Trigger class={buttonVariants({ variant: "ghost", size: "icon-sm" })} aria-label={m.devices_edit_named({ name: device.name })} title={m.devices_edit()}>
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
          <form id={`remove-device-${device.id}`} method="POST" action="?/removeDevice">
            <input type="hidden" name="device_id" value={device.id} />
            <AlertDialog.Root>
              <AlertDialog.Trigger
                type="button"
                class={buttonVariants({ variant: "ghost", size: "icon-sm", class: "hover:text-destructive focus-visible:text-destructive" })}
                aria-label={m.devices_remove_named({ name: device.name })}
                title={m.devices_remove()}
              >
                <TrashIcon size={16} />
              </AlertDialog.Trigger>
              <AlertDialog.Content>
                <AlertDialog.Header>
                  <AlertDialog.Title>{m.devices_remove_title({ name: device.name })}</AlertDialog.Title>
                  <AlertDialog.Description>
                    {m.devices_remove_text()}
                  </AlertDialog.Description>
                </AlertDialog.Header>
                <AlertDialog.Footer>
                  <AlertDialog.Cancel>{m.cancel()}</AlertDialog.Cancel>
                  <AlertDialog.Action variant="destructive" type="submit" form={`remove-device-${device.id}`}>
                    {m.devices_remove()}
                  </AlertDialog.Action>
                </AlertDialog.Footer>
              </AlertDialog.Content>
            </AlertDialog.Root>
          </form>
        </div>
      </li>
    {/each}
  </ManagementList>
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
  form {
    margin: 0;
  }
  .device-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
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
    color: var(--muted-foreground);
    font-size: var(--text-13);
    margin-top: -8px;
  }

  @media (max-width: 760px) {
    .device-actions {
      flex-wrap: wrap;
      justify-content: flex-start;
    }
  }
</style>
