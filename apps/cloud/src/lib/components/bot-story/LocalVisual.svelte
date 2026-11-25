<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon';
	import DeviceMobileIcon from 'phosphor-svelte/lib/DeviceMobileIcon';
	import FileHtmlIcon from 'phosphor-svelte/lib/FileHtmlIcon';
	import type { Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';
	import BotAvatarMark from './BotAvatarMark.svelte';

	let { story, step }: { story: Story; step: number } = $props();
	const fileOpen = $derived(step === 2);
	const settings = [m.story_local_notifications, m.story_local_digest, m.story_local_dark];
	const colors = ['#98971a', '#d79921', '#458588', '#b16286'];
</script>

<div class="window card">
	<div class="titlebar"><i></i><i></i><i></i><span class="mono">Dashboard</span></div>
	<div class="layout">
		<div class="chat">
			<div class="bar"><span class="avatar"><BotAvatarMark shape={story.shape} color={story.color} /></span>{story.name}</div>
			<div class="feed">
				<span class="msg user">{m.story_local_ask()}</span>
				<span class="msg bot">
					{m.story_local_reply()}<br />
					<span class="link" class:lit={!fileOpen}>localhost:5173 <ArrowSquareOutIcon size={13} /></span>
				</span>
				<span class="card file" class:lit={fileOpen}>
					<FileHtmlIcon size={18} /><span><b>palette.html</b><small class="mono">HTML · 4 KB</small></span>
				</span>
			</div>
		</div>
		<div class="preview">
			<div class="toolbar">
				{#if fileOpen}
					<FileHtmlIcon size={16} /><span class="address mono">palette.html</span>
				{:else}
					<span class="address mono">http://localhost:5173/</span><ArrowClockwiseIcon size={15} /><DeviceMobileIcon size={15} />
				{/if}
				<ArrowSquareOutIcon size={15} />
			</div>
			<div class="canvas">
				{#if fileOpen}
					<div class="site enter">
						<div class="body">
							<strong>{m.story_local_palette()}</strong>
							<div class="swatches">
								{#each colors as color (color)}
									<div><i style:background={color}></i><span>{color}</span></div>
								{/each}
							</div>
						</div>
					</div>
				{:else if step === 1}
					<div class="site enter">
						<div class="nav"><i></i>Atlas<span></span></div>
						<div class="body">
							<strong>{m.story_local_settings()}</strong>
							{#each settings as label, i (label)}
								<div class="row">{label()}<i class="switch" class:on={i !== 1}></i></div>
							{/each}
							<span class="save">{m.story_local_save()}</span>
						</div>
					</div>
				{/if}
			</div>
		</div>
	</div>
</div>

<style>
	.window { place-self: center; width: min(100%, 860px); min-width: 0; overflow: hidden; }
	.titlebar, .bar, .toolbar { display: flex; align-items: center; gap: 8px; height: 42px; padding-inline: 14px; border-bottom: 1px solid var(--border); }
	.titlebar { height: 34px; gap: 6px; }
	.titlebar i { width: 9px; height: 9px; border-radius: 50%; background: var(--border-strong); }
	.titlebar .mono { margin-left: 10px; }
	.layout { display: grid; grid-template-columns: minmax(0, 0.85fr) minmax(0, 1.15fr); }
	.chat, .preview, .feed { display: flex; flex-direction: column; min-width: 0; }
	.chat { border-right: 1px solid var(--border); }
	.bar { color: var(--heading); font-size: 13px; font-weight: 500; }
	.avatar { width: 22px; height: 22px; }
	.feed { flex: 1; justify-content: flex-end; gap: 8px; padding: 14px; }
	.link { display: inline-flex; align-items: center; gap: 4px; margin-top: 4px; color: var(--link); font: 500 12px var(--font-mono); text-decoration: underline; text-underline-offset: 3px; border-radius: 4px; }
	.link.lit { background: color-mix(in srgb, var(--primary) 14%, transparent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--primary) 14%, transparent); }
	.file { display: flex; align-items: center; gap: 10px; width: fit-content; padding: 8px 12px; color: var(--muted-foreground); }
	.file.lit { border-color: var(--primary); }
	.file > span { display: grid; }
	.file b { color: var(--heading); font-size: 13px; font-weight: 500; }
	.toolbar { gap: 10px; color: var(--muted-foreground); }
	.address { flex: 1; min-width: 0; padding: 4px 8px; border: 1px solid var(--border); border-radius: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.canvas { display: grid; flex: 1; min-height: 272px; padding: 12px; background: var(--muted); }
	.site { border-radius: 6px; background: white; color: #282828; font-size: 13px; box-shadow: 0 6px 18px -12px rgb(60 56 54 / 35%); }
	.nav { display: flex; align-items: center; gap: 8px; height: 34px; padding-inline: 14px; border-bottom: 1px solid #ebe8e1; font-weight: 600; }
	.nav i { width: 16px; height: 16px; border-radius: 5px; background: #d79921; }
	.nav span { margin-left: auto; width: 18px; height: 18px; border-radius: 50%; background: #b16286; }
	.body { display: grid; padding: 12px 16px; }
	.body strong { font-size: 17px; font-weight: 600; }
	.row { display: flex; align-items: center; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid #efece5; }
	.switch { width: 30px; height: 18px; padding: 2px; border-radius: 12px; background: #d5d2ca; }
	.switch::after { content: ''; display: block; width: 14px; height: 14px; border-radius: 50%; background: white; }
	.switch.on { background: #458588; }
	.switch.on::after { margin-left: auto; }
	.save { justify-self: start; margin-top: 12px; padding: 6px 12px; border-radius: 6px; color: white; background: #407e81; font-size: 12px; }
	.swatches { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 10px; }
	.swatches div { display: grid; gap: 4px; font: 11px var(--font-mono); }
	.swatches i { height: 60px; border-radius: 6px; }
	@media (max-width: 760px) {
		.layout { grid-template-columns: minmax(0, 1fr); }
		.chat { border-right: 0; border-bottom: 1px solid var(--border); }
		.feed { padding: 10px; }
		.toolbar { gap: 6px; padding-inline: 8px; }
	}
</style>
