/** Where a per-file action sits: in a file row, or over an image in a mosaic tile. */
export type FileActionLook = 'row' | 'overlay'

/** Button props shared by every per-file action, such as download and the Bot message menu, so they differ only in icon. */
export const fileActionButton = { variant: 'ghost', size: 'icon-sm' } as const

/** Plain icons in a row; dark glass circles over an image. */
export const fileActionClass: Record<FileActionLook, string> = {
  row: 'size-8 rounded-full text-muted-foreground hover:bg-background hover:text-foreground aria-expanded:bg-background aria-expanded:text-foreground',
  overlay: "rounded-full bg-black/55 text-white backdrop-blur-sm hover:bg-black/70 hover:text-white aria-expanded:bg-black/70 aria-expanded:text-white [&_svg:not([class*='size-'])]:size-3.5",
}
