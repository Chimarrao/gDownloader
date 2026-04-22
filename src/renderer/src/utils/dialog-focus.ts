export function dialogFocusableElements(root: HTMLElement | null): HTMLElement[] {
  if (!root) {
    return []
  }

  return Array.from(
    root.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((element) => !element.hasAttribute('hidden') && element.offsetParent !== null)
}

export function focusFirstDialogElement(root: HTMLElement | null): void {
  const [first] = dialogFocusableElements(root)
  if (first) {
    first.focus()
    return
  }
  root?.focus()
}

export function trapDialogTab(event: KeyboardEvent, root: HTMLElement | null): void {
  if (event.key !== 'Tab') {
    return
  }

  const focusable = dialogFocusableElements(root)
  if (focusable.length === 0) {
    event.preventDefault()
    root?.focus()
    return
  }

  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  const active = document.activeElement as HTMLElement | null

  if (event.shiftKey) {
    if (!active || active === first || !root?.contains(active)) {
      event.preventDefault()
      last.focus()
    }
    return
  }

  if (!active || active === last || !root?.contains(active)) {
    event.preventDefault()
    first.focus()
  }
}
