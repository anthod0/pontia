<script lang="ts">
	import BrainIcon from 'phosphor-svelte/lib/BrainIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import { storyByName, type Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();
	// The other Bots on the same device read the same USER.md and memory. They match the MacBook in DevicesVisual.
	const others = ['Juno', 'Kai', 'Ivy'].map(storyByName);
</script>

<div class="memory" style:--bot={story.color}>
	<div class="chat card">
		<div class="bar"><span class="avatar"><BotAvatarMark shape={story.shape} color={story.color} /></span>{story.name}</div>
		<div class="feed">
			<span class="msg user enter">{m.story_memory_ask()}</span>
			{#if step >= 1}<span class="msg bot enter">{m.story_memory_ack()}</span>{/if}
		</div>
	</div>

	<div class="device">
		<span class="device-label mono"><DesktopIcon size={13} />{m.story_memory_device()}</span>
		<div class="stores">
			<div class={['store card', step >= 1 && 'lit']}>
				<span class="store-title"><FileTextIcon size={14} />USER.md</span>
				<ul>
					{#if step >= 1}
						<li class="enter"><CheckIcon size={11} weight="bold" />{m.story_memory_pref_pnpm()}</li>
						<li class="enter" style:animation-delay="60ms"><CheckIcon size={11} weight="bold" />{m.story_memory_pref_short()}</li>
					{/if}
				</ul>
			</div>
			<div class={['store card', step >= 2 && 'lit']}>
				<span class="store-title"><BrainIcon size={14} />{m.story_memory_title()}</span>
				<ul>
					{#if step >= 2}<li class="enter"><CheckIcon size={11} weight="bold" />{m.story_memory_fact()}</li>{/if}
				</ul>
			</div>
		</div>
		<div class={['bots', step >= 3 && 'shared']}>
			<span class="bot self"><BotAvatarMark shape={story.shape} color={story.color} /></span>
			{#each others as other, i (other.name)}
				<span class="bot other" style:transition-delay="{i * 60}ms"><BotAvatarMark shape={other.shape} color={other.color} /></span>
			{/each}
		</div>
	</div>
</div>

<style>
	.memory { display: grid; grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.1fr); gap: 36px; align-items: center; }
	.chat { display: flex; flex-direction: column; height: 240px; overflow: hidden; border-radius: 12px; }
	.bar { display: flex; align-items: center; gap: 8px; height: 42px; padding-inline: 14px; border-bottom: 1px solid var(--border); font-size: 13px; font-weight: 500; color: var(--heading); }
	.avatar { width: 22px; height: 22px; }
	.feed { display: flex; flex: 1; flex-direction: column; justify-content: flex-end; gap: 8px; padding: 14px; }

	.device { position: relative; display: grid; gap: 18px; padding: 34px 18px 18px; border: 1px dashed var(--border-stronger); border-radius: 14px; }
	.device-label { position: absolute; top: 10px; left: 14px; display: inline-flex; align-items: center; gap: 6px; }
	.stores { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
	.store { display: grid; align-content: start; gap: 8px; min-height: 118px; padding: 12px 14px; }
	.store.lit { border-color: color-mix(in srgb, var(--bot) 55%, var(--border)); }
	.store-title { display: inline-flex; align-items: center; gap: 6px; font: 500 12px var(--font-mono); color: var(--heading); }
	.store ul { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }
	.store li { display: flex; align-items: baseline; gap: 6px; font-size: 12.5px; line-height: 1.45; color: var(--foreground); }
	.store li :global(svg) { flex-shrink: 0; color: var(--coverage-supported); }

	/* All Bots on the device share the stores; the others light up once Remy has written to them. */
	.bots { display: flex; justify-content: center; gap: 18px; }
	.bot { width: 40px; height: 40px; transition: opacity 160ms, filter 160ms, translate 160ms; }
	.bot.other { opacity: 0.35; filter: saturate(0.3); }
	.shared .bot.other { opacity: 1; filter: none; translate: 0 -4px; }

	@media (max-width: 760px) {
		.memory { grid-template-columns: 1fr; gap: 14px; }
		.chat { height: auto; }
		.feed { min-height: 104px; }
		.stores { grid-template-columns: 1fr; }
		.store { min-height: 0; }
		.store ul { min-height: 18px; }
		.bot { width: 32px; height: 32px; }
	}
</style>
