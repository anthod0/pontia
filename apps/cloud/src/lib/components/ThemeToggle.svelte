<script lang="ts">
	import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
	import MoonIcon from 'phosphor-svelte/lib/MoonIcon';
	import SunIcon from 'phosphor-svelte/lib/SunIcon';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { resolvedTheme, themePreference, type ThemePreference } from '$lib/theme';
	import { m } from '$lib/paraglide/messages';

	const options: { value: ThemePreference; label: () => string; icon: typeof SunIcon }[] = [
		{ value: 'system', label: m.theme_system, icon: DesktopIcon },
		{ value: 'light', label: m.theme_light, icon: SunIcon },
		{ value: 'dark', label: m.theme_dark, icon: MoonIcon }
	];
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger class="theme-trigger focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-label={m.theme_label()}>
		{#if $resolvedTheme === 'dark'}<MoonIcon size={16} />{:else}<SunIcon size={16} />{/if}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content align="end" class="w-32 min-w-0 p-1">
		<DropdownMenu.RadioGroup value={$themePreference} onValueChange={(value) => themePreference.set(value as ThemePreference)}>
			{#each options as option (option.value)}
				<DropdownMenu.RadioItem closeOnSelect value={option.value} class="py-1.5">
					<option.icon />
					{option.label()}
				</DropdownMenu.RadioItem>
			{/each}
		</DropdownMenu.RadioGroup>
	</DropdownMenu.Content>
</DropdownMenu.Root>

<style>
	:global(.theme-trigger) {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border: 1px solid transparent;
		border-radius: 4px;
		background: transparent;
		color: var(--foreground);
		transition: background 0.15s, color 0.15s;
	}
	:global(.theme-trigger:hover),
	:global(.theme-trigger[data-state='open']) {
		background: var(--muted);
		color: var(--primary);
	}
</style>
