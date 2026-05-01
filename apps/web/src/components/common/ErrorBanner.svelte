<script lang="ts">
  export let message: string | null = null;
</script>

{#if message}
  <div class="error" role="alert">{message}</div>
{/if}
