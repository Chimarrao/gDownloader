import { existsSync, mkdirSync, readdirSync } from 'fs'
import { basename, dirname, extname, join } from 'path'
import { spawn } from 'child_process'

const ARCHIVE_EXTS = new Set(['.zip', '.rar', '.7z', '.tar', '.gz', '.bz2', '.xz', '.zst'])
const MULTIPART_RAR = /\.part\d+\.rar$/i
const MULTIPART_7Z_FIRST = /\.7z\.001$/i
const MULTIPART_ZIP = /\.zip\.\d+$/i
const PART_001 = /\.001$/i

export function isExtractable(filePath: string): boolean {
  const ext = extname(filePath).toLowerCase()
  if (ARCHIVE_EXTS.has(ext)) return true
  if (MULTIPART_RAR.test(filePath)) return true
  if (MULTIPART_7Z_FIRST.test(filePath)) return true
  return false
}

function isFirstPart(filePath: string): boolean {
  if (PART_001.test(filePath)) return true
  if (/\.part0*1\.rar$/i.test(filePath)) return true
  if (MULTIPART_7Z_FIRST.test(filePath)) return true
  return false
}

/** Returns whether all expected parts of a multipart archive are present. */
export function allPartsReady(firstPartPath: string): boolean {
  const dir = dirname(firstPartPath)
  const base = basename(firstPartPath)

  if (/\.part0*1\.rar$/i.test(base)) {
    // Gather all .partNNN.rar siblings
    const prefix = base.replace(/\.part0*1\.rar$/i, '')
    const parts = readdirSync(dir).filter((f) => {
      const re = new RegExp(`^${escapeRegex(prefix)}\\.part\\d+\\.rar$`, 'i')
      return re.test(f)
    })
    if (parts.length === 0) return false
    // Check that all consecutive parts exist
    const nums = parts.map((p) => parseInt(p.match(/\.part(\d+)\.rar$/i)?.[1] ?? '0', 10))
    const max = Math.max(...nums)
    for (let i = 1; i <= max; i++) {
      const exists = parts.some((p) => {
        const n = parseInt(p.match(/\.part(\d+)\.rar$/i)?.[1] ?? '-1', 10)
        return n === i
      })
      if (!exists) return false
    }
    return true
  }

  if (PART_001.test(base)) {
    const prefix = base.replace(/\.001$/, '')
    const parts = readdirSync(dir).filter((f) => {
      const re = new RegExp(`^${escapeRegex(prefix)}\\.\\d{3}$`)
      return re.test(f)
    })
    if (parts.length < 2) return parts.length === 1
    const nums = parts.map((p) => parseInt(p.match(/\.(\d{3})$/)?.[1] ?? '0', 10))
    const max = Math.max(...nums)
    // Check no gaps
    for (let i = 1; i <= max; i++) {
      if (!nums.includes(i)) return false
    }
    return true
  }

  return true // not multipart
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

export interface AutoExtractResult {
  success: boolean
  outputDir?: string
  error?: string
  passwordUsed?: string
}

/** Try to extract an archive, optionally iterating through passwords. */
export async function autoExtract(
  archivePath: string,
  passwords: string[],
): Promise<AutoExtractResult> {
  if (!existsSync(archivePath)) {
    return { success: false, error: 'Arquivo não encontrado' }
  }

  const outputDir = join(dirname(archivePath), basename(archivePath, extname(archivePath)))
  mkdirSync(outputDir, { recursive: true })

  // First, try without password
  try {
    await runExtract(archivePath, outputDir, undefined)
    return { success: true, outputDir }
  } catch (err) {
    const msg = String(err)
    if (!isWrongPassword(msg) || passwords.length === 0) {
      return { success: false, error: msg }
    }
  }

  // Try each password
  for (const pwd of passwords) {
    try {
      await runExtract(archivePath, outputDir, pwd)
      return { success: true, outputDir, passwordUsed: pwd }
    } catch (err) {
      if (!isWrongPassword(String(err))) {
        return { success: false, error: String(err) }
      }
    }
  }

  return { success: false, error: 'WRONG_PASSWORD' }
}

function isWrongPassword(msg: string): boolean {
  return (
    msg.toLowerCase().includes('wrong password') ||
    msg.toLowerCase().includes('incorrect password') ||
    msg.toLowerCase().includes('cannot open encrypted') ||
    msg.includes('ERROR: Wrong password')
  )
}

async function runExtract(
  archivePath: string,
  outputDir: string,
  password: string | undefined,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const args = ['x', '-y', `-o${outputDir}`, archivePath]
    if (password) args.push(`-p${password}`)

    const proc = spawn('7z', args, { stdio: 'pipe' })
    let stderr = ''
    proc.stderr?.on('data', (d: Buffer) => { stderr += d.toString() })

    proc.on('close', (code) => {
      if (code === 0) {
        resolve()
      } else {
        reject(new Error(stderr || `7z exited with code ${code}`))
      }
    })

    proc.on('error', (err) => {
      reject(new Error(`Failed to spawn 7z: ${err.message}`))
    })
  })
}

export function shouldAutoExtractFile(filePath: string): boolean {
  return isExtractable(filePath) && (!isMultipart(filePath) || isFirstPart(filePath))
}

function isMultipart(filePath: string): boolean {
  return MULTIPART_RAR.test(filePath) || PART_001.test(filePath) || MULTIPART_ZIP.test(filePath)
}
