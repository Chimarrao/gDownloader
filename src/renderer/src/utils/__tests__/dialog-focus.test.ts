import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  dialogFocusableElements,
  focusFirstDialogElement,
  trapDialogTab,
} from '../dialog-focus'

interface FakeElement {
  focus: ReturnType<typeof vi.fn>
  hasAttribute: (name: string) => boolean
  offsetParent: object | null
}

function makeFocusable(options: { hidden?: boolean; visible?: boolean } = {}): HTMLElement {
  const hidden = options.hidden ?? false
  const visible = options.visible ?? true
  const element: FakeElement = {
    focus: vi.fn(),
    hasAttribute: (name: string) => hidden && name === 'hidden',
    offsetParent: visible ? {} : null,
  }
  return element as unknown as HTMLElement
}

function makeRoot(elements: HTMLElement[]): HTMLElement {
  return {
    querySelectorAll: () => elements,
    contains: (candidate: unknown) => elements.includes(candidate as HTMLElement),
    focus: vi.fn(),
  } as unknown as HTMLElement
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('dialog-focus helpers', () => {
  it('filtra elementos escondidos ou sem layout ao listar focáveis', () => {
    const visible = makeFocusable()
    const hidden = makeFocusable({ hidden: true })
    const detached = makeFocusable({ visible: false })
    const root = makeRoot([visible, hidden, detached])

    expect(dialogFocusableElements(root)).toEqual([visible])
  })

  it('foca o primeiro elemento disponível', () => {
    const first = makeFocusable()
    const second = makeFocusable()
    const root = makeRoot([first, second])

    focusFirstDialogElement(root)

    expect((first as unknown as FakeElement).focus).toHaveBeenCalledTimes(1)
    expect((second as unknown as FakeElement).focus).not.toHaveBeenCalled()
  })

  it('faz loop do tab para o primeiro quando o foco está no último', () => {
    const first = makeFocusable()
    const last = makeFocusable()
    const root = makeRoot([first, last])
    vi.stubGlobal('document', { activeElement: last })

    const event = {
      key: 'Tab',
      shiftKey: false,
      preventDefault: vi.fn(),
    } as unknown as KeyboardEvent

    trapDialogTab(event, root)

    expect(event.preventDefault).toHaveBeenCalledTimes(1)
    expect((first as unknown as FakeElement).focus).toHaveBeenCalledTimes(1)
  })

  it('faz loop reverso com shift+tab quando o foco está no primeiro', () => {
    const first = makeFocusable()
    const last = makeFocusable()
    const root = makeRoot([first, last])
    vi.stubGlobal('document', { activeElement: first })

    const event = {
      key: 'Tab',
      shiftKey: true,
      preventDefault: vi.fn(),
    } as unknown as KeyboardEvent

    trapDialogTab(event, root)

    expect(event.preventDefault).toHaveBeenCalledTimes(1)
    expect((last as unknown as FakeElement).focus).toHaveBeenCalledTimes(1)
  })
})
