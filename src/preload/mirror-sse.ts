export interface ParsedSseMessage {
  data: string
}

export function splitSseMessages(buffer: string): { messages: ParsedSseMessage[]; rest: string } {
  const normalized = buffer.replace(/\r\n/g, '\n')
  const chunks = normalized.split('\n\n')
  const rest = chunks.pop() ?? ''
  const messages = chunks
    .map((raw) => raw.trim())
    .filter((raw) => raw.length > 0 && !raw.startsWith(':'))
    .map((raw) => ({
      data: raw
        .split('\n')
        .filter((line) => line.startsWith('data:'))
        .map((line) => line.slice(5).trimStart())
        .join('\n'),
    }))
    .filter((message) => message.data.length > 0)

  return { messages, rest }
}

