/**
 * Prunes a list of captured rows after adding selected items to the download queue.
 *
 * Rules:
 * - Simple file row (no children or empty children array): removed if `row.selected === true`, else kept.
 * - Folder/multi-child row (info.children with length > 0):
 *   - ALL children chosen  → remove whole row
 *   - SOME children chosen → keep row with only NON-chosen children remaining
 *   - NO  children chosen  → keep intact
 *
 * A child is considered "chosen" when `child.selected !== false` (undefined = chosen by default).
 */
export function pruneCapturedRows<
  T extends {
    selected?: boolean
    info?: { children?: Array<{ selected?: boolean }> | null } | null
  },
>(rows: T[]): T[] {
  const result: T[] = []

  for (const row of rows) {
    const children = row.info?.children

    // Multi-child (folder) row
    if (children && children.length > 0) {
      const keptChildren = children.filter((child) => child.selected === false)

      if (keptChildren.length === children.length) {
        // No children were chosen → keep row intact
        result.push(row)
      } else if (keptChildren.length > 0) {
        // Some chosen → keep row with only non-chosen children
        result.push({ ...row, info: { ...row.info, children: keptChildren } })
      }
      // else: all chosen → drop entire row (don't push)
      continue
    }

    // Simple file row (no children or empty children array)
    if (row.selected !== true) {
      result.push(row)
    }
  }

  return result
}
