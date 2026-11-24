<script lang="ts">
	import BlueprintIcon from 'phosphor-svelte/lib/BlueprintIcon';
	import CodeIcon from 'phosphor-svelte/lib/CodeIcon';
	import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon';
	import { claude, codex, pi, type Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';
	import AgentBadge from './AgentBadge.svelte';

	let { story, step }: { story: Story; step: number } = $props();
	const roles = [
		{ title: m.story_models_design, icon: BlueprintIcon, agent: claude, model: 'Claude Opus' },
		{ title: m.story_models_code, icon: CodeIcon, agent: codex, model: 'GPT-5.3-Codex' },
		{ title: m.story_models_research, icon: MagnifyingGlassIcon, agent: pi, model: 'Gemini Flash' }
	];
</script>

<ul class="roles" style:--bot={story.color}>
	{#each roles as role, i (role.agent.client)}
		<li class="card role">
			<span class="icon"><role.icon size={22} /></span>
			<strong>{role.title()}</strong>
			<div class="selection">
				{#if step >= i}
					<div class="enter"><AgentBadge agent={role.agent} /><b>{role.model}</b></div>
				{/if}
			</div>
		</li>
	{/each}
</ul>

<style>
	.roles { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); align-self: center; gap: 12px; padding: 0; margin: 0; list-style: none; }
	.role { display: grid; grid-template-rows: auto 1fr auto; justify-items: start; gap: 14px; padding: 18px; color: var(--heading); }
	.icon { display: grid; place-items: center; width: 38px; height: 38px; border-radius: 10px; color: var(--heading); background: color-mix(in srgb, var(--bot) 20%, var(--surface)); }
	strong { font-size: 15px; }
	.selection { width: 100%; min-height: 50px; border-top: 1px solid var(--border); padding-top: 10px; }
	.selection div { display: grid; gap: 5px; }
	b { font: 12px var(--font-mono); }
	@media (max-width: 760px) {
		.roles { grid-template-columns: 1fr; gap: 8px; }
		.role { grid-template: auto auto / auto 1fr; align-items: center; gap: 6px 12px; padding: 12px; }
		.icon { grid-row: span 2; }
		.selection { grid-column: 2; min-height: 18px; padding: 0; border: 0; }
		.selection div { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 12px; }
	}
</style>
