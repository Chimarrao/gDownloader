import { describe, expect, it } from 'vitest'

import { splitSseMessages } from '../mirror-sse'

describe('splitSseMessages', () => {
  it('separa múltiplas mensagens e preserva resto do buffer', () => {
    const payload = [
      'data: {"type":"start","total":2}',
      '',
      'data: {"type":"progress","current":1}',
      '',
      'data: {"type":"done"}',
      '',
      'data: {"type":"trailing"',
    ].join('\n')

    const parsed = splitSseMessages(payload)

    expect(parsed.messages).toHaveLength(3)
    expect(parsed.messages[0].data).toContain('"type":"start"')
    expect(parsed.messages[1].data).toContain('"type":"progress"')
    expect(parsed.messages[2].data).toContain('"type":"done"')
    expect(parsed.rest).toContain('"type":"trailing"')
  })

  it('ignora comentários e concatena linhas data', () => {
    const payload = [
      ':keep-alive',
      '',
      'data: {"type":"log",',
      'data: "payload":"ok"}',
      '',
      '',
    ].join('\n')

    const parsed = splitSseMessages(payload)
    expect(parsed.messages).toHaveLength(1)
    expect(parsed.messages[0].data).toBe('{"type":"log",\n"payload":"ok"}')
  })
})
