<script lang="ts">
  import { untrack, type Snippet } from 'svelte';
  import { provideDeviceRuntime } from './context';
  import type { DeviceRuntime } from './runtime';
  let { device, children }: { device: DeviceRuntime; children: Snippet } = $props();
  provideDeviceRuntime(untrack(() => device));
</script>

{@render children()}
