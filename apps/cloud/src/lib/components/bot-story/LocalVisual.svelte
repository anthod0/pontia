<script lang="ts">
	import FileHtmlIcon from 'phosphor-svelte/lib/FileHtmlIcon';
	import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon';
	import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon';
	import type { Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();
</script>

<div class="previews" style:--bot={story.color}>
	<div class="card preview enter">
		<div class="source"><FileHtmlIcon size={18} />{m.story_local_file()}</div>
		<div class="address mono">prototype.html</div>
		<div class="result">
			<strong>{m.story_local_prototype()}</strong>
			<div class="swatches"><i></i><i></i><i></i><i></i></div>
		</div>
	</div>
	{#if step >= 1}
		<div class="card preview enter">
			<div class="source"><GlobeIcon size={18} />{m.story_local_server()}</div>
			<div class="address mono">localhost:5173</div>
			<div class="result">
				<strong>{m.story_local_tests()}</strong>
				<span class="passed"><CheckCircleIcon size={22} />12 / 12</span>
			</div>
		</div>
	{/if}
</div>

<style>
	.previews { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; align-content: center; }
	.preview { overflow: hidden; }
	.source, .address { padding: 10px 16px; border-bottom: 1px solid var(--border); }
	.source { display: flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 500; color: var(--heading); }
	.address { background: var(--surface); }
	.result { display: grid; gap: 12px; padding: 20px 16px; color: var(--heading); }
	strong { font-size: 15px; }
	.swatches, .passed { display: flex; align-items: center; gap: 5px; height: 24px; }
	.swatches i { flex: 1; height: 100%; border-radius: 4px; background: var(--bot); }
	.swatches i:nth-child(2) { background: #d79921; }
	.swatches i:nth-child(3) { background: #458588; }
	.swatches i:nth-child(4) { background: #b16286; }
	.passed { color: var(--coverage-supported); font: 14px var(--font-mono); }
	@media (max-width: 760px) {
		.previews { grid-template-columns: 1fr; grid-template-rows: repeat(2, 1fr); gap: 8px; }
		.source, .address { padding: 7px 12px; }
		.result { padding: 12px; }
	}
</style>
