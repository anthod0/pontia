<script lang="ts">
  import { token } from '../../stores/auth';
  import { lastConnectionError, reconnectCount, sseStatus, streamedSessionId } from '../../stores/connection';
  import { statusError, statusMessage } from '../../stores/ui';
</script>

<header>
  <div>
    <h1>llmparty Dashboard</h1>
    <p>Svelte + Vite agent control-plane validation panel</p>
  </div>
  <label>
    API token
    <input type="password" bind:value={$token} placeholder="Bearer token" autocomplete="off" />
  </label>
  <div class="status-stack">
    <span class:error={$statusError}>{$statusMessage}</span>
    <small class:error={$sseStatus === 'error'}>
      SSE: {$sseStatus}{ $streamedSessionId ? ` · ${$streamedSessionId}` : ''}{ $reconnectCount ? ` · reconnects ${$reconnectCount}` : ''}
      {#if $lastConnectionError} · {$lastConnectionError}{/if}
    </small>
  </div>
</header>
