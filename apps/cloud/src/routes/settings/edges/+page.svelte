<script lang="ts">
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import { onMount } from "svelte";
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import * as Table from "$lib/components/ui/table";
  import { ParaglideMessage } from "@inlang/paraglide-js-svelte";
  import { m } from "$lib/paraglide/messages";
  import { getLocale, localizeHref } from "$lib/paraglide/runtime";
  import type { ActionData, PageData } from "./$types";

  type HealthStatus = "checking" | "healthy" | "unreachable";

  let { data, form }: { data: PageData; form: ActionData } = $props();
  let healthByEdge = $state<Record<string, HealthStatus>>({});

  onMount(() => {
    const controller = new AbortController();
    const edgeIds = [...data.ownedEdges, ...data.publicEdges].map((edge) => edge.id);

    void fetch("/settings/edges/health", {
      headers: { Accept: "application/json" },
      signal: controller.signal,
    })
      .then(async (response) => {
        if (!response.ok) throw new Error("Health request failed");
        const result = (await response.json()) as {
          edges?: Record<string, "healthy" | "unreachable">;
        };
        for (const edgeId of edgeIds) {
          healthByEdge[edgeId] = result.edges?.[edgeId] ?? "unreachable";
        }
      })
      .catch(() => {
        if (controller.signal.aborted) return;
        for (const edgeId of edgeIds) healthByEdge[edgeId] = "unreachable";
      });

    return () => controller.abort();
  });

  async function copyEdgeId(event: MouseEvent, edgeId: string) {
    if ((event.target as HTMLElement).closest("a, button, input, form")) return;
    try {
      await navigator.clipboard.writeText(edgeId);
    } catch {
      // This hidden shortcut has no visible error state.
    }
  }
</script>

{#snippet healthBadge(edgeId: string)}
  {@const status = healthByEdge[edgeId] ?? "checking"}
  <Badge
    variant={status === "unreachable" ? "destructive" : status === "healthy" ? "secondary" : "outline"}
    class={status === "healthy" ? "health-healthy" : undefined}
    aria-live="polite"
  >
    {status === "checking" ? m.edges_health_checking() : status === "healthy" ? m.edges_health_healthy() : m.edges_health_unreachable()}
  </Badge>
{/snippet}

<svelte:head>
  <title>{m.edges_title()}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<h1>{m.settings_edges()}</h1>
<p>{m.edges_intro()}</p>
{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}

<section aria-labelledby="your-edges">
  <h2 id="your-edges">{m.edges_yours()}</h2>
  {#if data.ownedEdges.length > 0}
    <Table.Root class="table-fixed">
      <Table.Header>
        <Table.Row>
          <Table.Head class="w-[22%]">{m.edges_name()}</Table.Head>
          <Table.Head>{m.edges_address()}</Table.Head>
          <Table.Head class="w-28">{m.edges_created()}</Table.Head>
          <Table.Head class="w-28">{m.edges_health()}</Table.Head>
          <Table.Head class="w-20"><span class="visually-hidden">{m.edges_manage()}</span></Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each data.ownedEdges as edge (edge.id)}
          <Table.Row ondblclick={(event) => copyEdgeId(event, edge.id)}>
            <Table.Cell class="max-w-0">
              <div class="truncate" title={edge.name}>{edge.name}</div>
            </Table.Cell>
            <Table.Cell class="max-w-0">
              <div class="truncate" title={edge.tunnelUrl}>{edge.tunnelUrl}</div>
            </Table.Cell>
            <Table.Cell>{new Date(edge.createdAt).toLocaleDateString(getLocale())}</Table.Cell>
            <Table.Cell>{@render healthBadge(edge.id)}</Table.Cell>
            <Table.Cell>
              <div class="row-actions">
                <Dialog.Root>
                  <Dialog.Trigger
                    class="row-action"
                    aria-label={m.edges_rename_named({ name: edge.name })}
                    title={m.edges_rename()}
                  >
                    <PencilSimpleIcon size={16} />
                  </Dialog.Trigger>
                  <Dialog.Content>
                    <Dialog.Header>
                      <Dialog.Title>{m.edges_rename_named({ name: edge.name })}</Dialog.Title>
                      <Dialog.Description>{m.edges_rename_text()}</Dialog.Description>
                    </Dialog.Header>
                    <form class="rename-dialog-form" method="POST" action="?/rename">
                      <input type="hidden" name="edge_id" value={edge.id} />
                      <Input
                        name="name"
                        aria-label={m.edges_name_label()}
                        value={edge.name}
                        maxlength={100}
                        required
                      />
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
                <AlertDialog.Root>
                  <AlertDialog.Trigger
                    class="row-action delete-action"
                    aria-label={m.edges_delete_named({ name: edge.name })}
                    title={m.edges_delete()}
                  >
                    <TrashIcon size={16} />
                  </AlertDialog.Trigger>
                  <AlertDialog.Content>
                    <AlertDialog.Header>
                      <AlertDialog.Title>{m.edges_delete_title({ name: edge.name })}</AlertDialog.Title>
                      <AlertDialog.Description>
                        {m.edges_delete_text()}
                      </AlertDialog.Description>
                    </AlertDialog.Header>
                    <AlertDialog.Footer>
                      <AlertDialog.Cancel>{m.cancel()}</AlertDialog.Cancel>
                      <AlertDialog.Action type="submit" form={`delete-edge-${edge.id}`}>
                        {m.edges_delete_confirm()}
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
    <p>{m.edges_none()}</p>
  {/if}
</section>

<section aria-labelledby="public-edges">
  <h2 id="public-edges">{m.edges_public()}</h2>
  {#if data.publicEdges.length > 0}
    <Table.Root class="table-fixed">
      <Table.Header>
        <Table.Row>
          <Table.Head class="w-[30%]">{m.edges_name()}</Table.Head>
          <Table.Head>{m.edges_address()}</Table.Head>
          <Table.Head class="w-28">{m.edges_health()}</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each data.publicEdges as edge (edge.id)}
          <Table.Row ondblclick={(event) => copyEdgeId(event, edge.id)}>
            <Table.Cell class="max-w-0">
              <div class="truncate" title={edge.name}>{edge.name}</div>
            </Table.Cell>
            <Table.Cell class="max-w-0">
              <div class="truncate" title={edge.tunnelUrl}>{edge.tunnelUrl}</div>
            </Table.Cell>
            <Table.Cell>{@render healthBadge(edge.id)}</Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
  {:else}
    <p>{m.edges_public_none()}</p>
  {/if}
</section>

<section aria-labelledby="deploy-edge">
  <h2 id="deploy-edge">{m.edges_deploy_heading()}</h2>
  <p>{m.edges_deploy_text()}</p>
  <p>
    <ParaglideMessage message={m.edges_deploy_retry}>
      {#snippet code({ children })}<code>{@render children?.()}</code>{/snippet}
    </ParaglideMessage>
  </p>

  <form class="deployment-form" method="POST" action={localizeHref("/settings/edges/deploy")}>
    <Button type="submit">{m.edges_deploy_generate()}</Button>
    <p class="agreement">
      <ParaglideMessage message={m.edges_deploy_agreement}>
        {#snippet agreement({ children })}<a href="https://letsencrypt.org/repository/" target="_blank" rel="noreferrer">{@render children?.()}</a>{/snippet}
      </ParaglideMessage>
    </p>
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
  .deployment-form {
    display: flex;
    align-items: flex-start;
    flex-direction: column;
    gap: 12px;
    margin-top: 20px;
  }
  .agreement {
    max-width: 620px;
    margin: 0;
  }
  .agreement a {
    color: var(--primary);
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  code {
    border-radius: 4px;
    background: var(--muted);
    padding: 2px 5px;
    color: var(--foreground);
  }
  :global(.health-healthy) {
    border-color: color-mix(in oklab, var(--success) 35%, transparent);
    background: color-mix(in oklab, var(--success) 12%, transparent);
    color: var(--success);
  }
  .row-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
  }
  .row-actions :global(.row-action) {
    display: inline-flex;
    width: 28px;
    height: 28px;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
  }
  .row-actions :global(.row-action:hover),
  .row-actions :global(.row-action:focus-visible) {
    background: var(--muted);
    color: var(--foreground);
  }
  .row-actions :global(.delete-action:hover),
  .row-actions :global(.delete-action:focus-visible) {
    color: var(--color-destructive);
  }
  .rename-dialog-form {
    display: grid;
    gap: 16px;
  }
</style>
