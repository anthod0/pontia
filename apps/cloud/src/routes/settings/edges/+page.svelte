<script lang="ts">
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import * as Table from "$lib/components/ui/table";
  import type { ActionData, PageData } from "./$types";

  let { data, form }: { data: PageData; form: ActionData } = $props();

  async function copyEdgeId(event: MouseEvent, edgeId: string) {
    if ((event.target as HTMLElement).closest("a, button, input, form")) return;
    try {
      await navigator.clipboard.writeText(edgeId);
    } catch {
      // This hidden shortcut has no visible error state.
    }
  }
</script>

<svelte:head>
  <title>Edge settings — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>Edges</h1>
<p>Deploy and manage self-hosted Edge registrations.</p>
{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}

<section aria-labelledby="your-edges">
  <h2 id="your-edges">Your edges</h2>
  {#if data.ownedEdges.length > 0}
    <Table.Root>
      <Table.Header>
        <Table.Row>
          <Table.Head>Name</Table.Head>
          <Table.Head>Address</Table.Head>
          <Table.Head>Created</Table.Head>
          <Table.Head><span class="visually-hidden">Manage</span></Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each data.ownedEdges as edge (edge.id)}
          <Table.Row ondblclick={(event) => copyEdgeId(event, edge.id)}>
            <Table.Cell>{edge.name}</Table.Cell>
            <Table.Cell>{edge.tunnelUrl}</Table.Cell>
            <Table.Cell>{new Date(edge.createdAt).toLocaleString()}</Table.Cell>
            <Table.Cell>
              <div class="row-actions">
                <Dialog.Root>
                  <Dialog.Trigger class="row-action">Rename</Dialog.Trigger>
                  <Dialog.Content>
                    <Dialog.Header>
                      <Dialog.Title>Rename {edge.name}</Dialog.Title>
                      <Dialog.Description>Choose a new display name for this edge.</Dialog.Description>
                    </Dialog.Header>
                    <form class="rename-dialog-form" method="POST" action="?/rename">
                      <input type="hidden" name="edge_id" value={edge.id} />
                      <Input
                        name="name"
                        aria-label="Edge name"
                        value={edge.name}
                        maxlength={100}
                        required
                      />
                      <Dialog.Footer>
                        <Dialog.Close>
                          {#snippet child({ props })}
                            <Button variant="outline" {...props}>Cancel</Button>
                          {/snippet}
                        </Dialog.Close>
                        <Button type="submit">Save</Button>
                      </Dialog.Footer>
                    </form>
                  </Dialog.Content>
                </Dialog.Root>
                <AlertDialog.Root>
                  <AlertDialog.Trigger class="row-action">Delete</AlertDialog.Trigger>
                  <AlertDialog.Content>
                    <AlertDialog.Header>
                      <AlertDialog.Title>Delete {edge.name}?</AlertDialog.Title>
                      <AlertDialog.Description>
                        This permanently removes the edge registration and associated devices.
                      </AlertDialog.Description>
                    </AlertDialog.Header>
                    <AlertDialog.Footer>
                      <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
                      <AlertDialog.Action type="submit" form={`delete-edge-${edge.id}`}>
                        Delete edge
                      </AlertDialog.Action>
                    </AlertDialog.Footer>
                  </AlertDialog.Content>
                </AlertDialog.Root>
                <form id={`delete-edge-${edge.id}`} method="POST" action="?/delete">
                  <input type="hidden" name="edge_id" value={edge.id} />
                </form>
              </div>
            </Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
  {:else}
    <p>You haven't deployed any edges yet. Generate a command below to get started.</p>
  {/if}
</section>

<section aria-labelledby="public-edges">
  <h2 id="public-edges">Public edges</h2>
  {#if data.publicEdges.length > 0}
    <Table.Root>
      <Table.Header>
        <Table.Row>
          <Table.Head>Name</Table.Head>
          <Table.Head>Address</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each data.publicEdges as edge (edge.id)}
          <Table.Row ondblclick={(event) => copyEdgeId(event, edge.id)}>
            <Table.Cell>{edge.name}</Table.Cell>
            <Table.Cell>{edge.tunnelUrl}</Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
  {:else}
    <p>No public edges are available.</p>
  {/if}
</section>

<section aria-labelledby="deploy-edge">
  <h2 id="deploy-edge">Deploy a self-hosted edge</h2>
  <p>Generate a one-time command, then run it on your public Linux server.</p>

  <form method="POST" action="/settings/edges/deploy">
    <p class="agreement">
      By generating this command, you agree to the
      <a href="https://letsencrypt.org/repository/" target="_blank" rel="noreferrer">
        Let's Encrypt Subscriber Agreement</a
      >.
    </p>
    <Button type="submit">Generate deployment command</Button>
  </form>
</section>

<style>
  section {
    margin-top: 40px;
  }
  h2 {
    margin-bottom: 8px;
    font-size: 18px;
  }
  section > p,
  .agreement {
    color: var(--muted-foreground);
    font-size: 14px;
    line-height: 1.7;
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  .agreement {
    max-width: 620px;
    margin-block: 20px;
  }
  .row-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 14px;
  }
  .row-actions :global(.row-action) {
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .rename-dialog-form {
    display: grid;
    gap: 16px;
  }
</style>
