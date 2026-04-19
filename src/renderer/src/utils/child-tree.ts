import type { DownloadChild } from '../../../shared/types'
import { DownloadStatus } from '../../../shared/constants'

export interface DerivedChildNode<T extends DownloadChild = DownloadChild> {
  key: string
  name: string
  path: string
  size: number
  mimeType?: string
  isFolder: boolean
  sourceUrl?: string
  bytesDownloaded: number
  speedBps: number
  etaSec: number
  status?: DownloadStatus
  fileCount: number
  depth: number
  children: DerivedChildNode<T>[]
  original?: T
}

function normalizeSegments<T extends DownloadChild>(child: T): string[] {
  const rawPath = (child.path?.trim() || child.filename.trim()).replace(/^\/+|\/+$/g, '')
  const segments = rawPath.split('/').filter(Boolean)

  if (segments.length === 0) {
    return [child.filename]
  }

  if (segments[segments.length - 1] !== child.filename) {
    segments.push(child.filename)
  }

  return segments
}

function createFolderNode<T extends DownloadChild>(name: string, path: string): DerivedChildNode<T> {
  return {
    key: `folder:${path}`,
    name,
    path,
    size: 0,
    isFolder: true,
    bytesDownloaded: 0,
    speedBps: 0,
    etaSec: 0,
    fileCount: 0,
    depth: 0,
    children: [],
  }
}

function createLeafNode<T extends DownloadChild>(child: T, path: string): DerivedChildNode<T> {
  return {
    key: `file:${path}:${child.sourceUrl ?? child.filename}`,
    name: child.filename,
    path,
    size: child.size,
    mimeType: child.mimeType,
    isFolder: false,
    sourceUrl: child.sourceUrl,
    bytesDownloaded: child.bytesDownloaded ?? 0,
    speedBps: child.speedBps ?? 0,
    etaSec: child.etaSec ?? 0,
    status: child.status,
    fileCount: 1,
    depth: 0,
    children: [],
    original: child,
  }
}

function insertNode<T extends DownloadChild>(
  siblings: DerivedChildNode<T>[],
  child: T,
  segments: string[],
  currentPath = ''
): void {
  if (segments.length === 0) {
    return
  }

  if (segments.length === 1) {
    const leafPath = currentPath ? `${currentPath}/${segments[0]}` : segments[0]
    siblings.push(createLeafNode(child, leafPath))
    return
  }

  const folderName = segments[0]
  const folderPath = currentPath ? `${currentPath}/${folderName}` : folderName
  let folder = siblings.find((node) => node.isFolder && node.path === folderPath)
  if (!folder) {
    folder = createFolderNode(folderName, folderPath)
    siblings.push(folder)
  }

  insertNode(folder.children, child, segments.slice(1), folderPath)
}

function finalizeTree<T extends DownloadChild>(node: DerivedChildNode<T>): void {
  if (!node.isFolder) {
    return
  }

  for (const child of node.children) {
    finalizeTree(child)
  }

  node.children.sort((a, b) => {
    if (a.isFolder !== b.isFolder) {
      return a.isFolder ? -1 : 1
    }
    return a.name.localeCompare(b.name, 'pt-BR', { sensitivity: 'base' })
  })

  node.size = node.children.reduce((sum, child) => sum + child.size, 0)
  node.bytesDownloaded = node.children.reduce((sum, child) => sum + child.bytesDownloaded, 0)
  node.speedBps = node.children.reduce((sum, child) => sum + child.speedBps, 0)
  node.fileCount = node.children.reduce((sum, child) => sum + child.fileCount, 0)
  node.etaSec =
    node.speedBps > 0 && node.size > node.bytesDownloaded
      ? Math.ceil((node.size - node.bytesDownloaded) / node.speedBps)
      : 0

  if (node.children.length === 0) {
    node.status = DownloadStatus.Pending
    return
  }

  if (node.children.every((child) => child.status === DownloadStatus.Complete)) {
    node.status = DownloadStatus.Complete
  } else if (node.children.some((child) => child.status === DownloadStatus.Downloading)) {
    node.status = DownloadStatus.Downloading
  } else if (node.children.some((child) => child.status === DownloadStatus.Error)) {
    node.status = DownloadStatus.Error
  } else if (node.children.some((child) => child.status === DownloadStatus.Paused)) {
    node.status = DownloadStatus.Paused
  } else if (node.children.some((child) => child.status === DownloadStatus.Cancelled)) {
    node.status = DownloadStatus.Cancelled
  } else {
    node.status = DownloadStatus.Pending
  }
}

export function buildChildTree<T extends DownloadChild>(children: T[]): DerivedChildNode<T>[] {
  const roots: DerivedChildNode<T>[] = []

  for (const child of children) {
    insertNode(roots, child, normalizeSegments(child))
  }

  for (const node of roots) {
    finalizeTree(node)
  }

  roots.sort((a, b) => {
    if (a.isFolder !== b.isFolder) {
      return a.isFolder ? -1 : 1
    }
    return a.name.localeCompare(b.name, 'pt-BR', { sensitivity: 'base' })
  })

  return roots
}

export function flattenChildTree<T extends DownloadChild>(nodes: DerivedChildNode<T>[], depth = 0): DerivedChildNode<T>[] {
  const flattened: DerivedChildNode<T>[] = []

  for (const node of nodes) {
    flattened.push({
      ...node,
      depth,
    })
    if (node.children.length > 0) {
      flattened.push(...flattenChildTree(node.children, depth + 1))
    }
  }

  return flattened
}
