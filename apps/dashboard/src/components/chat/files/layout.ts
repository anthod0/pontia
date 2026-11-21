/** A list card shows every file up to this many; longer lists collapse. */
export const LIST_FULL = 6;
/** Rows a collapsed list card shows. */
export const LIST_COLLAPSED = 4;
/** Tiles a collapsed image mosaic shows; the last one counts the rest. */
export const MOSAIC_TILES = 5;

/** Whether the file at `index` is hidden until the card is expanded. */
export function hiddenUntilExpanded(index: number, count: number, mosaic: boolean): boolean {
  if (mosaic) return count > MOSAIC_TILES && index >= MOSAIC_TILES - 1;
  return count > LIST_FULL && index >= LIST_COLLAPSED;
}
