// Transcrição local de áudio (sem API paga, sem chave) usada pelo bypass
// automático de reCAPTCHA v2 / hCaptcha via desafio de áudio — mesmo padrão
// de "venv Python gerenciado, instalado sob demanda" já usado pelos solvers
// de Turnstile em turnstile-service.ts, mas isolado aqui porque não é um
// solver de Turnstile e usa uma dependência diferente (faster-whisper).
import { execFile } from 'child_process'
import { existsSync, mkdirSync, writeFileSync } from 'fs'
import { join } from 'path'

function findPython(): string {
  const candidates = ['python3.12', 'python3', 'python']
  for (const cmd of candidates) {
    try {
      const out = require('child_process').execSync(`${cmd} --version`, { encoding: 'utf8', timeout: 3000 })
      if (out.includes('Python 3.')) return cmd
    } catch {
      // tenta o próximo
    }
  }
  return 'python3'
}

function run(cmd: string, args: string[], timeoutMs = 180_000): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout: timeoutMs, maxBuffer: 20 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) reject(err)
      else resolve({ stdout: stdout.toString(), stderr: stderr.toString() })
    })
  })
}

function rootDir(userDataPath: string): string {
  return join(userDataPath, 'audio-captcha-solver')
}

function venvDir(userDataPath: string): string {
  return join(rootDir(userDataPath), 'venv')
}

function venvPython(userDataPath: string): string {
  return process.platform === 'win32'
    ? join(venvDir(userDataPath), 'Scripts', 'python.exe')
    : join(venvDir(userDataPath), 'bin', 'python')
}

function scriptPath(userDataPath: string): string {
  return join(rootDir(userDataPath), 'transcribe.py')
}

// tiny.en via faster-whisper (CTranslate2, wheels pré-compilados — sem
// precisar de toolchain de compilação local) — modelo baixado sob demanda
// pela própria lib na primeira transcrição (~75MB, cacheado pelo huggingface_hub).
const TRANSCRIBE_SCRIPT = [
  'import sys',
  'from faster_whisper import WhisperModel',
  '',
  'model = WhisperModel("tiny.en", device="cpu", compute_type="int8")',
  'segments, _ = model.transcribe(sys.argv[1], language="en", beam_size=5)',
  'print(" ".join(seg.text for seg in segments).strip())',
  '',
].join('\n')

let readyPromise: Promise<void> | null = null

async function ensureReady(userDataPath: string): Promise<void> {
  if (readyPromise) return readyPromise
  readyPromise = (async () => {
    mkdirSync(rootDir(userDataPath), { recursive: true })
    writeFileSync(scriptPath(userDataPath), TRANSCRIBE_SCRIPT, 'utf8')
    const python = venvPython(userDataPath)
    if (!existsSync(python)) {
      const sysPython = findPython()
      await run(sysPython, ['-m', 'venv', venvDir(userDataPath)], 60_000)
      await run(python, ['-m', 'pip', 'install', '--upgrade', 'pip'], 120_000)
      // av>=14 renomeou/removeu o kwarg metadata_errors que o faster-whisper
      // 1.2.x ainda usa em decode_audio — confirmado quebrando na prática
      // (TypeError: open() got an unexpected keyword argument 'metadata_errors').
      await run(python, ['-m', 'pip', 'install', 'faster-whisper', 'av<14'], 300_000)
    }
  })().catch((err) => {
    readyPromise = null // permite retentar numa próxima chamada em vez de ficar preso num erro de instalação transitório
    throw err
  })
  return readyPromise
}

export async function transcribeAudio(userDataPath: string, audioFilePath: string): Promise<string> {
  await ensureReady(userDataPath)
  const python = venvPython(userDataPath)
  const { stdout } = await run(python, [scriptPath(userDataPath), audioFilePath], 120_000)
  return stdout.trim()
}
