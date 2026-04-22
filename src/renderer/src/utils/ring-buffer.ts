export function pushRingBuffer(
  current: number[],
  nextValue: number,
  capacity: number,
): number[] {
  if (capacity <= 0) {
    return []
  }

  if (current.length < capacity) {
    return [...current, nextValue]
  }

  return [...current.slice(1), nextValue]
}

