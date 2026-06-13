import type { ContextUsageView } from '../api/types';

export function formatTokenCount(value: number): string {
  if (value >= 1_000_000) return `${Math.round(value / 100_000) / 10}m`;
  if (value >= 1_000) return `${Math.round(value / 1_000)}k`;
  return String(value);
}

export function contextUsageRatio(usage: ContextUsageView): number | null {
  if (usage.usage_ratio !== null) return usage.usage_ratio;
  if (usage.used_tokens !== null && usage.max_tokens !== null && usage.max_tokens > 0) return usage.used_tokens / usage.max_tokens;
  return null;
}

export function contextUsageSummary(usage: ContextUsageView, options: { includeConfidence?: boolean } = {}): string {
  const ratio = contextUsageRatio(usage);
  const percent = ratio === null ? null : `${Math.round(ratio * 100)}%`;
  const usageLabel = usage.used_tokens !== null && usage.max_tokens !== null
    ? `${formatTokenCount(usage.used_tokens)} / ${formatTokenCount(usage.max_tokens)}`
    : usage.used_tokens !== null
      ? formatTokenCount(usage.used_tokens)
      : 'unknown';
  const parts = [usageLabel, percent];
  if (options.includeConfidence !== false) parts.push(usage.confidence);
  return `Context ${parts.filter(Boolean).join(' · ')}`;
}
