<script lang="ts">
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import type { SessionState } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';
	let { state }: { state: SessionState } = $props();
</script>

<span class={['state', state]}>
	{#if state === 'running'}<span class="dot"></span>{m.story_state_running()}
	{:else if state === 'done'}<CheckIcon size={11} weight="bold" />{m.story_state_done()}{/if}
</span>
