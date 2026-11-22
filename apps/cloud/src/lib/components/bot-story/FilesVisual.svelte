<script lang="ts">
	import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon';
	import EyeIcon from 'phosphor-svelte/lib/EyeIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import FileZipIcon from 'phosphor-svelte/lib/FileZipIcon';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import type { Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();
	// Step 2 opens the screenshot, step 3 the text report. The archive is a download.
	const preview = $derived(step === 2 ? 'image' : step >= 3 ? 'text' : null);
</script>

{#snippet screenshot(large: boolean)}
	<span class={['shot', large && 'large']}>
		<span class="shot-bar"></span>
		<span class="shot-side"></span>
		<span class="shot-body"><i></i><i></i><i class="short"></i><b></b></span>
	</span>
{/snippet}

<div class="files" style:--bot={story.color}>
	<div class="chat card">
		<div class="bar"><span class="avatar"><BotAvatarMark shape={story.shape} color={story.color} /></span>{story.name}</div>
		<div class="feed">
			<span class="msg user enter">{m.story_files_ask()}</span>
			{#if step >= 1}
				<span class="msg bot enter">{m.story_files_reply()}</span>
				<div class="attachments enter">
					<span class={['tile', preview === 'image' && 'active']}>{@render screenshot(false)}</span>
					<span class={['file card', preview === 'text' && 'active']}>
						<FileTextIcon size={18} /><span class="file-name">report.md<small>4 KB</small></span><EyeIcon size={14} />
					</span>
					<span class="file card">
						<FileZipIcon size={18} /><span class="file-name">settings-build.zip<small>2.1 MB</small></span><DownloadSimpleIcon size={14} />
					</span>
				</div>
			{/if}
		</div>
	</div>

	<div class="preview card">
		<div class="bar">
			<EyeIcon size={14} />{m.story_files_preview()}
			{#if preview}<span class="mono">{preview === 'image' ? 'settings-light.png' : 'report.md'}</span>{/if}
		</div>
		<div class="preview-body">
			{#if preview === 'image'}
				<div class="enter">{@render screenshot(true)}</div>
			{:else if preview === 'text'}
				<div class="doc enter">
					<b># {m.story_files_report_title()}</b>
					<span>- {m.story_files_report_line1()}</span>
					<span>- {m.story_files_report_line2()}</span>
				</div>
			{/if}
		</div>
	</div>
</div>

<style>
	.files { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 28px; align-items: center; }
	.chat, .preview { display: flex; flex-direction: column; height: 364px; overflow: hidden; border-radius: 12px; }
	.bar { display: flex; align-items: center; gap: 8px; height: 42px; flex-shrink: 0; padding-inline: 14px; border-bottom: 1px solid var(--border); font-size: 13px; font-weight: 500; color: var(--heading); }
	.bar .mono { margin-left: auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.avatar { width: 22px; height: 22px; }
	.feed { display: flex; flex: 1; flex-direction: column; justify-content: flex-end; gap: 8px; padding: 14px; }

	.attachments { display: grid; grid-template-columns: 96px minmax(0, 1fr); grid-template-rows: auto auto; gap: 6px; max-width: 92%; }
	.tile { grid-row: 1 / 3; display: block; border-radius: 9px; outline: 2px solid transparent; outline-offset: 2px; transition: outline-color 250ms; }
	.tile.active, .file.active { outline: 2px solid var(--primary); outline-offset: 2px; }
	.file { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: 9px; color: var(--muted-foreground); }
	.file > :global(svg:first-child) { flex-shrink: 0; color: var(--bot); }
	.file-name { display: grid; flex: 1; min-width: 0; font-size: 12px; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.file-name small { font: 10px var(--font-mono); color: var(--muted-foreground); }

	/* A drawn screenshot of a settings page: top bar, sidebar, and a few rows. */
	.shot { display: grid; grid-template-columns: 26% 1fr; grid-template-rows: 14% 1fr; aspect-ratio: 4 / 3; width: 100%; overflow: hidden; border: 1px solid var(--border); border-radius: 8px; background: var(--background); }
	.shot-bar { grid-column: 1 / 3; background: color-mix(in srgb, var(--bot) 70%, var(--background)); }
	.shot-side { background: var(--muted); }
	.shot-body { display: flex; flex-direction: column; gap: 9%; padding: 10% 9%; }
	.shot-body i { height: 6%; min-height: 3px; border-radius: 3px; background: var(--border-strong); }
	.shot-body i.short { width: 60%; }
	.shot-body b { width: 36%; height: 12%; min-height: 6px; margin-top: auto; border-radius: 4px; background: var(--button); }
	.shot.large { border-radius: 10px; }

	.preview-body { display: grid; flex: 1; place-items: center; padding: 22px; background: var(--surface); }
	.preview-body > div { width: 100%; }
	.doc { display: grid; gap: 8px; align-self: start; padding: 16px 18px; border: 1px solid var(--border); border-radius: 8px; background: var(--background); font: 12.5px/1.6 var(--font-mono); color: var(--foreground); }
	.doc b { font-weight: 600; color: var(--heading); }

	@media (max-width: 760px) {
		.files { grid-template-columns: 1fr; gap: 14px; }
		.chat { height: 300px; }
		.preview { height: 230px; }
		.preview-body { padding: 14px; }
		.preview-body > div { width: min(100%, 250px); }
		.attachments { grid-template-columns: 84px minmax(0, 1fr); max-width: 100%; }
	}
</style>
