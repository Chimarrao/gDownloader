// Extensões de arquivos compactados suportados pela extração automática.
const ARCHIVE_EXTENSIONS = [
  '.rar', '.zip', '.7z', '.tar', '.tar.gz', '.tgz', '.tar.bz2', '.tbz2',
  '.tar.xz', '.txz', '.tar.zst', '.gz', '.bz2', '.xz', '.zst',
]

/**
 * Retorna true se o nome do arquivo é um archive (para o qual faz sentido pedir
 * senha de extração). Cobre extensões diretas e partes multipart (.partN.rar,
 * .7z.001, .zNN, .rNN).
 */
export function isArchiveFilename(filename: string | undefined | null): boolean {
  if (!filename) return false
  const lower = filename.toLowerCase().trim()
  if (ARCHIVE_EXTENSIONS.some((ext) => lower.endsWith(ext))) return true
  if (/\.(7z|zip|rar)\.\d{3}$/.test(lower)) return true
  if (/\.(r|z)\d{2}$/.test(lower)) return true
  return false
}
