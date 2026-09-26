export type SelectionRectangle = { left: number; top: number; width: number; height: number };
type SelectionPoint = { x: number; y: number };
type InventoryTileBounds = { assetId: number; left: number; top: number; right: number; bottom: number };

export function getSelectionRectangle(start: SelectionPoint, current: SelectionPoint): SelectionRectangle {
  return {
    left: Math.min(start.x, current.x),
    top: Math.min(start.y, current.y),
    width: Math.abs(current.x - start.x),
    height: Math.abs(current.y - start.y),
  };
}

export function selectInventoryAssets(
  rectangle: SelectionRectangle,
  tiles: readonly InventoryTileBounds[],
  baseIds: ReadonlySet<number>,
): Set<number> {
  const selected = new Set(baseIds);
  for (const tile of tiles) {
    if (tile.left < rectangle.left + rectangle.width && tile.right > rectangle.left &&
        tile.top < rectangle.top + rectangle.height && tile.bottom > rectangle.top) {
      selected.add(tile.assetId);
    }
  }
  return selected;
}
