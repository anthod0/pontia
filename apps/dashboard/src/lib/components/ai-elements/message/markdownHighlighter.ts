import { createHighlighterCoreSync, type LanguageRegistration, type ThemeRegistrationAny } from 'shiki/core';
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript';
import bash from 'shiki/langs/bash.mjs';
import c from 'shiki/langs/c.mjs';
import cpp from 'shiki/langs/cpp.mjs';
import css from 'shiki/langs/css.mjs';
import go from 'shiki/langs/go.mjs';
import html from 'shiki/langs/html.mjs';
import java from 'shiki/langs/java.mjs';
import javascript from 'shiki/langs/javascript.mjs';
import json from 'shiki/langs/json.mjs';
import jsx from 'shiki/langs/jsx.mjs';
import markdown from 'shiki/langs/markdown.mjs';
import python from 'shiki/langs/python.mjs';
import ruby from 'shiki/langs/ruby.mjs';
import rust from 'shiki/langs/rust.mjs';
import sql from 'shiki/langs/sql.mjs';
import svelte from 'shiki/langs/svelte.mjs';
import toml from 'shiki/langs/toml.mjs';
import tsx from 'shiki/langs/tsx.mjs';
import typescript from 'shiki/langs/typescript.mjs';
import xml from 'shiki/langs/xml.mjs';
import yaml from 'shiki/langs/yaml.mjs';
import gruvboxLight from 'shiki/themes/gruvbox-light-soft.mjs';

const highlighter = createHighlighterCoreSync({
  engine: createJavaScriptRegexEngine(),
  langs: [
    bash,
    c,
    cpp,
    css,
    go,
    html,
    java,
    javascript,
    json,
    jsx,
    markdown,
    python,
    ruby,
    rust,
    sql,
    svelte,
    toml,
    tsx,
    typescript,
    xml,
    yaml,
  ] as unknown as LanguageRegistration[],
  themes: [gruvboxLight] as unknown as ThemeRegistrationAny[],
});

const loadedLanguages = new Set(highlighter.getLoadedLanguages());

export function highlightMarkdownCode(code: string, language: string): string {
  const normalizedLanguage = language.trim().toLowerCase();
  if (!normalizedLanguage || !loadedLanguages.has(normalizedLanguage)) return fallbackCode(code);

  try {
    return highlighter.codeToHtml(code, {
      lang: normalizedLanguage,
      theme: 'gruvbox-light-soft',
    });
  } catch {
    return fallbackCode(code);
  }
}

function fallbackCode(code: string): string {
  return `<pre class="shiki shiki-fallback"><code>${escapeHtml(code)}</code></pre>`;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}
