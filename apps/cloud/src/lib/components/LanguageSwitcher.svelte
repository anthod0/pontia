<script lang="ts">
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import TranslateIcon from 'phosphor-svelte/lib/TranslateIcon';
	import { page } from '$app/state';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { localeInfo } from '$lib/locales';
	import { m } from '$lib/paraglide/messages';
	import { deLocalizeHref, getLocale, locales, localizeHref } from '$lib/paraglide/runtime';

	const path = $derived(deLocalizeHref(`${page.url.pathname}${page.url.search}${page.url.hash}`));
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger class="theme-trigger" aria-label={m.language_label()}>
		<TranslateIcon size={16} />
	</DropdownMenu.Trigger>
	<DropdownMenu.Content align="end" class="w-max min-w-0 rounded-md p-1 shadow-md">
		{#each locales as locale (locale)}
			<DropdownMenu.Item class="rounded-sm py-1.5 text-[13px] whitespace-nowrap">
				{#snippet child({ props })}
					<!-- A full reload loads the messages and server data of the new locale. -->
					<a
						{...props}
						href={localizeHref(path, { locale })}
						hreflang={locale}
						lang={locale}
						aria-current={locale === getLocale() ? 'true' : undefined}
						data-sveltekit-reload
					>
						<span class="flex-1">{localeInfo[locale].name}</span>
						{#if locale === getLocale()}<CheckIcon />{/if}
					</a>
				{/snippet}
			</DropdownMenu.Item>
		{/each}
	</DropdownMenu.Content>
</DropdownMenu.Root>
