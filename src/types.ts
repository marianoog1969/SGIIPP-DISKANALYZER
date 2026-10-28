export type SortKey = "size" | "name" | "files" | "mtime";

export interface NodeDto {
  id: number;
  parent: number;
  name: string;
  isDir: boolean;
  size: number;
  files: number;
  mtime: number;
}

export interface ChildrenPage {
  items: NodeDto[];
  total: number;
  restCount: number;
  restSize: number;
}

export interface LiveItem {
  name: string;
  isDir: boolean;
  size: number;
  files: number;
}

export interface ScanProgress {
  files: number;
  dirs: number;
  bytes: number;
  errors: number;
  current: string;
  elapsedMs: number;
  /** Entradas de primer nivel más grandes hasta ahora. */
  top: LiveItem[];
  topTotal: number;
  topRestSize: number;
}

/** Lo mínimo de un elemento para seleccionarlo, borrarlo o abrir su menú. */
export interface Pick {
  id: number;
  name: string;
  isDir: boolean;
  size: number;
  /** Carpeta que lo contiene, si se conoce (para «ir a su carpeta»). */
  parent?: number;
}

export interface SearchItem {
  id: number;
  parent: number;
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  files: number;
  mtime: number;
}

export interface SearchResult {
  items: SearchItem[];
  count: number;
  totalSize: number | null;
}

export interface SearchArgs {
  text: string;
  minSize: number;
  olderThanDays: number | null;
  exts: string;
  kind: "files" | "dirs" | "all";
  limit: number;
}

export interface ScanDone {
  rootId: number;
  root: string;
  size: number;
  files: number;
  dirs: number;
  errors: number;
  elapsedMs: number;
  cancelled: boolean;
}

export interface TopFile {
  id: number;
  parent: number;
  name: string;
  path: string;
  size: number;
  mtime: number;
}

export interface ExtStat {
  ext: string;
  count: number;
  size: number;
}

export interface ScanErr {
  path: string;
  message: string;
}

export interface DeleteItem {
  id: number;
  name: string;
  ok: boolean;
  error: string | null;
}

export interface DeleteResult {
  results: DeleteItem[];
  freed: number;
  size: number;
  files: number;
  dirs: number;
  cancelled: boolean;
}

export interface DeleteProgress {
  files: number;
  bytes: number;
  errors: number;
  current: string;
  elapsedMs: number;
  index: number;
  total: number;
  name: string;
  phase: "delete" | "refresh";
}

export interface RefreshResult {
  size: number;
  files: number;
  dirs: number;
  removed: boolean;
}

export interface Drive {
  path: string;
  kind: string;
}

export interface DriveSpace {
  total: number;
  free: number;
}

export interface ExportResult {
  path: string;
  rows: number;
  truncated: boolean;
}

export interface ReportDefaults {
  center: string;
  unit: string;
  fromFile: boolean;
}
