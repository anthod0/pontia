<script lang="ts">
  import { token } from '../../stores/auth';
  import { statusError, statusMessage } from '../../stores/ui';
</script>

<header>
  <div>
    <h1>llmparty Dashboard</h1>
    <p>Svelte + Vite External API control panel</p>
  </div>
  <label>
    API token
    <input type="password" bind:value={$token} placeholder="Bearer token" autocomplete="off" />
  </label>
  <span class:error={$statusError}>{$statusMessage}</span>
</header>
