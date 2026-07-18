<script lang="ts">
  import Layout from '../../src/routes/+layout.svelte';
</script>

<Layout>
  {#snippet children()}
    <div>Dashboard route content</div>
  {/snippet}
</Layout>
