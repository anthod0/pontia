<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as Table from "$lib/components/ui/table";
  import AuthLayout from "$lib/components/AuthLayout.svelte";
  import type { PageData } from "./$types";

  let { data }: { data: PageData } = $props();
</script>

<svelte:head>
  <title>Self-hosted edges — Pontia</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<AuthLayout wide>
  <h1>Self-hosted edges</h1>

  <section aria-labelledby="your-edges">
    <h2 id="your-edges">Your edges</h2>
    {#if data.edges.length > 0}
      <Table.Root>
        <Table.Header>
          <Table.Row>
            <Table.Head>Name</Table.Head>
            <Table.Head>Address</Table.Head>
            <Table.Head>Created</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each data.edges as edge (edge.id)}
            <Table.Row>
              <Table.Cell>{edge.name}</Table.Cell>
              <Table.Cell>{edge.tunnelUrl}</Table.Cell>
              <Table.Cell>{new Date(edge.createdAt).toLocaleString()}</Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </Table.Root>
    {:else}
      <p>You haven't deployed any edges yet. Generate a command below to get started.</p>
    {/if}
  </section>

  <h2>Deploy a self-hosted edge</h2>
  <p>Generate a one-time command, then run it on your public Linux server.</p>

  <form method="POST" action="/edges/deploy">
    <p class="agreement">
      By generating this command, you agree to the
      <a href="https://letsencrypt.org/repository/" target="_blank" rel="noreferrer">
        Let's Encrypt Subscriber Agreement</a
      >.
    </p>
    <Button type="submit">Generate deployment command</Button>
  </form>
</AuthLayout>

<style>
  section {
    margin-block: 28px;
  }

  h2 {
    font-size: 18px;
    margin-bottom: 8px;
  }

  .agreement {
    margin-block: 20px;
    max-width: 620px;
  }

</style>
