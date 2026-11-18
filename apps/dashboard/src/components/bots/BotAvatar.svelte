<script lang="ts">
  import type { BotAvatarColor, BotAvatarShape } from '../../api/types'
  import { cn } from '$lib/utils'
  import { avatarColors, avatarShapes } from './avatar'

  let { shape, color, class: className }: {
    shape: BotAvatarShape
    color: BotAvatarColor
    class?: string
  } = $props()
</script>

<svg viewBox="9.9 12.4 83 83" class={cn('shrink-0 overflow-visible', className)} aria-hidden="true">
  <path d={avatarShapes[shape]} fill={avatarColors[color]}
    stroke={shape === 'onigiri' ? avatarColors[color] : undefined}
    stroke-width={shape === 'onigiri' ? 28 : undefined} stroke-linejoin="round" />
  <g fill="#fff">
    <rect x="33" y="45" width="10" height="18" rx="5" />
    <rect x="57" y="45" width="10" height="18" rx="5" />
  </g>
</svg>
