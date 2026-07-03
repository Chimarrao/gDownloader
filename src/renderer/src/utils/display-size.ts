interface SizedChild {
  size?: number | null
}

/**
 * Tamanho a exibir para um item. Prioriza o tamanho do próprio item; se for 0,
 * cai para os filhos: soma quando é pasta (todos serão baixados), ou o maior
 * filho quando são formatos alternativos do mesmo arquivo (ex.: YouTube).
 */
export function effectiveSize(
  itemSize: number | null | undefined,
  children?: SizedChild[] | null,
  isFolder = false,
): number {
  const base = Number(itemSize ?? 0)
  if (base > 0) return base
  const sizes = (children ?? []).map((c) => Number(c.size ?? 0)).filter((n) => n > 0)
  if (sizes.length === 0) return 0
  return isFolder ? sizes.reduce((a, b) => a + b, 0) : Math.max(...sizes)
}
