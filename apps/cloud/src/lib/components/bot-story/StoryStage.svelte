<script lang="ts">
	import type { Story } from '$lib/bot-stories';
	import StoryChat from './StoryChat.svelte';
	import SessionsVisual from './SessionsVisual.svelte';
	import ProfilesVisual from './ProfilesVisual.svelte';
	import WorktreesVisual from './WorktreesVisual.svelte';
	import MemoryVisual from './MemoryVisual.svelte';
	import DevicesVisual from './DevicesVisual.svelte';

	let { story, step }: { story: Story; step: number } = $props();
	const visuals = {
		sessions: SessionsVisual,
		profiles: ProfilesVisual,
		worktrees: WorktreesVisual,
		memory: MemoryVisual,
		devices: DevicesVisual
	};
	const Visual = $derived(story.visual && visuals[story.visual]);
</script>

<div class="stage" aria-hidden="true">
	{#if story.chat}
		<StoryChat {story} chat={story.chat} {step} />
	{:else if Visual}
		<Visual {story} {step} />
	{/if}
</div>

<style>
	.stage { display: grid; min-height: 420px; padding: 28px 36px; border: 1px solid var(--border); border-radius: 14px; background: radial-gradient(circle at 1px 1px, var(--border) 1px, transparent 0) 0 0 / 22px 22px, var(--surface); }

	.stage :global(.card) { border: 1px solid var(--border); border-radius: 10px; background: var(--background); box-shadow: 0 16px 32px -26px rgb(60 56 54 / 35%); transition: border-color 250ms, opacity 250ms; }
	.stage :global(.enter) { animation: story-enter 360ms var(--ease-out) both; }
	.stage :global(.state) { display: inline-flex; align-items: center; gap: 6px; font: 11px var(--font-mono); white-space: nowrap; }
	.stage :global(.state.running) { color: var(--color-busy); }
	.stage :global(.state.done) { color: var(--coverage-supported); }
	.stage :global(.dot) { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
	.stage :global(.state.running .dot) { animation: story-pulse 1.6s ease-out infinite; }
	.stage :global(.msg) { width: fit-content; max-width: 86%; padding: 8px 12px; border-radius: 14px; font-size: 13px; line-height: 1.45; }
	.stage :global(.msg.user) { align-self: flex-end; border-bottom-right-radius: 4px; background: var(--button); color: white; }
	.stage :global(.msg.bot) { align-self: flex-start; border: 1px solid var(--border); border-bottom-left-radius: 4px; background: var(--surface); color: var(--foreground); }
	.stage :global(.mono) { font: 11px var(--font-mono); color: var(--muted-foreground); }

	@keyframes -global-story-enter { from { opacity: 0; translate: 0 6px; } }
	@keyframes -global-story-pulse { 0% { box-shadow: 0 0 0 0 rgb(215 153 33 / 50%); } 100% { box-shadow: 0 0 0 6px rgb(215 153 33 / 0%); } }
	@media (prefers-reduced-motion: reduce) { .stage :global(.enter), .stage :global(.state.running .dot) { animation: none; } }

	@media (max-width: 760px) {
		.stage { min-height: 0; padding: 14px; }
	}
</style>
