<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import DemoGitChanges from './DemoGitChanges.svelte';
	import type { DemoGit } from './mockData';

	/** The Session chat's Git panel. */
	let { git, onclose }: { git: DemoGit; onclose: () => void } = $props();
	let element = $state<HTMLElement>();
	let changes = $state<DemoGitChanges>();

	// Opening the panel moves focus into it, as in the Dashboard.
	$effect(() => element?.focus());
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside bind:this={element} tabindex="-1" onkeydown={(event) => changes?.keydown(event)} class="absolute inset-0 z-20 flex min-w-0 flex-col bg-surface @5xl:static @5xl:z-auto @5xl:w-[22rem] @5xl:shrink-0 @5xl:border-l @5xl:border-border outline-none" aria-label="Git changes">
	<DemoGitChanges bind:this={changes} {git} {onclose} hint="Alt G toggle · ↑↓ select · Enter open">
		{#snippet header()}
			<header class="flex h-12 shrink-0 items-center gap-1 border-b border-border px-2">
				<h2 class="git-heading flex h-full items-center gap-1.5 border-b-2 border-primary px-2.5 text-sm font-medium text-heading">
					<GitBranchIcon class="size-4" />Git
					<span class="text-xs font-normal text-muted-foreground tabular-nums">{git.changes.length}</span>
				</h2>
				<span class="flex-1"></span>
				<span class="flex size-7 items-center justify-center" aria-hidden="true"><ArrowClockwiseIcon class="size-4" /></span>
				<button type="button" class="flex size-7 items-center justify-center rounded-md text-foreground hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" aria-label="Close" title="Close (Alt G)" onclick={onclose}><XIcon class="size-4" /></button>
			</header>
		{/snippet}
	</DemoGitChanges>
</aside>

<style>
	/* The site's section headings style every h2; the panel keeps the Dashboard's size. */
	.git-heading {
		font-size: 14px;
		line-height: 20px;
		letter-spacing: normal;
	}
</style>
