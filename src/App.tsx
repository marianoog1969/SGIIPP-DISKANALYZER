import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import Treemap from "./Treemap";
import Manual from "./Manual";
import donateQr from "./assets/donate-qr.png";

const DONATE_URL = "https://www.paypal.me/marianoog";
import ContextMenu, { type MenuEntries, type MenuItem } from "./ContextMenu";
import { Icon } from "./Icons";
import { CATEGORY_INFO, categoryOf } from "./colors";
import { fmtBytes, fmtDate, fmtDur, fmtInt, fmtPct, joinPath, pct, shortenPath } from "./format";
import type {
  ChildrenPage,
  DeleteProgress,
  DeleteResult,
  Drive,
  ExtStat,
  NodeDto,
  Pick,
  DriveSpace,
  ReportDefaults,
  ScanDone,
  ScanErr,
  ScanProgress,
  SearchArgs,
  SearchItem,
  SearchResult,
  SortKey,
  TopFile,
} from "./types";

type Phase = "idle" | "scanning" | "done";
type Tab = "folders" | "search" | "files" | "ext" | "errors";

const LS_LAST = "espacio.ultima";
const LS_RECENT = "espacio.recientes";
const LS_THREADS = "espacio.hilos";
const LS_CENTER = "espacio.centro";
const LS_UNIT = "espacio.servicio";
const LS_THEME = "espacio.tema";

type Theme = "auto" | "light" | "dark";
const THEME_NEXT: Record<Theme, Theme> = { auto: "light", light: "dark", dark: "auto" };
const THEME_LABEL: Record<Theme, string> = { auto: "Automático", light: "Claro", dark: "Oscuro" };

function loadTheme(): Theme {
  const v = loadText(LS_THEME);
  return v === "light" || v === "dark" ? v : "auto";
}

/** Color de la barra de % según el peso de la fila. */
function barTone(p: number): string {
  return p >= 40 ? "hot" : p >= 15 ? "warm" : "";
}

const DRIVE_KIND: Record<string, string> = {
  local: "local",
  red: "red",
  extraible: "extraíble",
  cd: "óptica",
  ram: "RAM",
  otro: "",
};

function loadText(key: string): string {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function loadRecent(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(LS_RECENT) ?? "[]");
    return Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
  } catch {
    return [];
  }
}

const AGES: [string, string][] = [
  ["", "Cualquiera"],
  ["30", "Más de 1 mes"],
  ["180", "Más de 6 meses"],
  ["365", "Más de 1 año"],
  ["730", "Más de 2 años"],
  ["1825", "Más de 5 años"],
];
const UNITS: Record<string, number> = { KB: 1024, MB: 1024 ** 2, GB: 1024 ** 3 };
const SEARCH_LIMIT = 500;
const GIB = 1024 ** 3;
/** Por encima de este tamaño, borrar exige escribir ELIMINAR. */
const STRONG_LOCAL = 5 * GIB;
const STRONG_NET = 1 * GIB;
const noop = () => {};

interface SearchForm {
  text: string;
  size: string;
  unit: string;
  age: string;
  exts: string;
  kind: SearchArgs["kind"];
}
const EMPTY_SEARCH: SearchForm = { text: "", size: "", unit: "MB", age: "", exts: "", kind: "files" };

const pickOf = (n: { id: number; name: string; isDir: boolean; size: number; parent?: number }): Pick => ({
  id: n.id,
  name: n.name,
  isDir: n.isDir,
  size: n.size,
  parent: n.parent,
});

interface Ctx {
  x: number;
  y: number;
  p: Pick;
}

function dirOf(f: { path: string; name: string }): string {
  return f.path.slice(0, f.path.length - f.name.length).replace(/[\\/]$/, "");
}

export default function App() {
  const [path, setPath] = useState(() => localStorage.getItem(LS_LAST) ?? "");
  const [threads, setThreads] = useState(() => Number(localStorage.getItem(LS_THREADS)) || 16);
  const [recent, setRecent] = useState<string[]>(loadRecent);
  const [drives, setDrives] = useState<Drive[]>([]);

  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [done, setDone] = useState<ScanDone | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const [tab, setTab] = useState<Tab>("folders");
  const [cur, setCur] = useState(0);
  const [crumbs, setCrumbs] = useState<NodeDto[]>([]);
  const [page, setPage] = useState<ChildrenPage | null>(null);
  const [tm, setTm] = useState<ChildrenPage | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "size", desc: true });
  const [checked, setChecked] = useState<Map<number, Pick>>(new Map());
  const [confirmDel, setConfirmDel] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [reload, setReload] = useState(0);
  const [delProgress, setDelProgress] = useState<DeleteProgress | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const [sq, setSq] = useState<SearchForm>(EMPTY_SEARCH);
  const [sres, setSres] = useState<SearchResult | null>(null);
  const [searching, setSearching] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const [focusSearch, setFocusSearch] = useState(0);
  const [ctx, setCtx] = useState<Ctx | null>(null);
  const liveIds = useRef(new Map<string, number>());
  const [typed, setTyped] = useState("");
  const [showManual, setShowManual] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [hoverId, setHoverId] = useState<number | null>(null);
  const [spaces, setSpaces] = useState<Record<string, DriveSpace | null>>({});
  const [showSettings, setShowSettings] = useState(false);
  const [names, setNames] = useState(() => ({ center: loadText(LS_CENTER), unit: loadText(LS_UNIT) }));
  const [draft, setDraft] = useState({ center: "", unit: "" });
  const [defaults, setDefaults] = useState<ReportDefaults>({
    center: "Centro de Inserción Social «Guillermo Miranda» · Murcia",
    unit: "Servicios de Informática",
    fromFile: false,
  });
  const [notice, setNotice] = useState<{ text: string; path: string } | null>(null);
  const [expMenu, setExpMenu] = useState<{ x: number; y: number; kind: "folder" | "search" } | null>(null);
  const [exporting, setExporting] = useState(false);
  const anchorRef = useRef<number | null>(null);

  const [topFiles, setTopFiles] = useState<TopFile[] | null>(null);
  const [exts, setExts] = useState<ExtStat[] | null>(null);
  const [errs, setErrs] = useState<ScanErr[] | null>(null);

  const flash = useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast(null), 1800);
  }, []);

  // Unidades disponibles
  useEffect(() => {
    api
      .drives()
      .then((ds) => {
        setDrives(ds);
        // Capacidad de cada unidad, una a una: una unidad de red lenta no retrasa a las demás.
        ds.forEach((d) =>
          api
            .driveSpace(d.path)
            .then((s) => setSpaces((o) => ({ ...o, [d.path]: s })))
            .catch(() => setSpaces((o) => ({ ...o, [d.path]: null }))),
        );
      })
      .catch(() => {});
    api.reportDefaults().then(setDefaults).catch(() => {});
  }, []);

  // Eventos del escáner
  useEffect(() => {
    const subs = [
      listen<ScanProgress>("scan-progress", (e) => setProgress(e.payload)),
      listen<ScanDone>("scan-done", (e) => {
        setDone(e.payload);
        setCur(e.payload.rootId);
        setPhase("done");
      }),
      listen<string>("scan-error", (e) => {
        setError(e.payload);
        setPhase("idle");
      }),
      listen<DeleteProgress>("delete-progress", (e) => setDelProgress(e.payload)),
      listen<DeleteResult>("delete-done", (e) => {
        const r = e.payload;
        setDone((d) => (d ? { ...d, size: r.size, files: r.files, dirs: r.dirs } : d));
        setTopFiles(null);
        setExts(null);
        const failed = r.results.filter((x) => !x.ok);
        if (failed.length > 0) {
          setError(
            "No se pudo eliminar del todo: " +
              failed.map((f) => `${f.name} (${f.error})`).join("; ") +
              ". La lista ya muestra lo que queda en disco.",
          );
        } else if (r.cancelled) {
          flash("Borrado detenido");
        } else {
          flash(`Eliminado: ${fmtBytes(r.freed)} liberados`);
        }
        setChecked(new Map());
        setDeleting(false);
        setConfirmDel(false);
        setDelProgress(null);
        setReload((n) => n + 1);
      }),
      listen<string>("delete-error", (e) => {
        setError(e.payload);
        setDeleting(false);
        setConfirmDel(false);
        setDelProgress(null);
        setReload((n) => n + 1);
      }),
    ];
    return () => {
      subs.forEach((p) => p.then((off) => off()));
    };
  }, [flash]);

  const fetchFail = useCallback((e: unknown) => {
    const m = String(e);
    if (m.includes("no encontrada")) setCur(0);
    else setError(m);
  }, []);

  // Datos de la carpeta actual: miga de pan + treemap
  useEffect(() => {
    if (phase !== "done") return;
    let alive = true;
    Promise.all([api.ancestors(cur), api.children(cur, "size", true, 250)])
      .then(([a, t]) => {
        if (!alive) return;
        setCrumbs(a);
        setTm(t);
        setChecked(new Map());
      })
      .catch((e) => alive && fetchFail(e));
    return () => {
      alive = false;
    };
  }, [phase, cur, reload]);

  // Tabla de la carpeta actual (con el orden elegido)
  useEffect(() => {
    if (phase !== "done") return;
    let alive = true;
    api
      .children(cur, sort.key, sort.desc, 1000)
      .then((p) => alive && setPage(p))
      .catch((e) => alive && fetchFail(e));
    return () => {
      alive = false;
    };
  }, [phase, cur, sort, reload]);

  // Pestañas que se calculan al abrirlas
  useEffect(() => {
    if (phase !== "done") return;
    const fail = (e: unknown) => setError(String(e));
    if (tab === "files" && !topFiles) api.topFiles(300).then(setTopFiles).catch(fail);
    if (tab === "ext" && !exts) api.extensions(200).then(setExts).catch(fail);
    if (tab === "errors" && !errs) api.errors().then(setErrs).catch(fail);
  }, [phase, tab, topFiles, exts, errs]);

  const searchArgs = (limit: number): SearchArgs => {
    const num = parseFloat(sq.size.replace(",", "."));
    return {
      text: sq.text,
      minSize: Number.isFinite(num) && num > 0 ? Math.round(num * (UNITS[sq.unit] ?? 1)) : 0,
      olderThanDays: sq.age ? Number(sq.age) : null,
      exts: sq.exts,
      kind: sq.kind,
      limit,
    };
  };

  // Búsqueda: se lanza sola un instante después de tocar un filtro.
  useEffect(() => {
    if (phase !== "done" || tab !== "search") return;
    let alive = true;
    const args = searchArgs(SEARCH_LIMIT);
    setSearching(true);
    const h = window.setTimeout(() => {
      api
        .search(args)
        .then((r) => alive && setSres(r))
        .catch((e) => alive && setError(String(e)))
        .finally(() => alive && setSearching(false));
    }, 250);
    return () => {
      alive = false;
      window.clearTimeout(h);
    };
  }, [phase, tab, sq, reload]);

  useEffect(() => {
    if (tab === "search") searchRef.current?.focus();
  }, [tab, focusSearch]);

  useEffect(() => {
    setChecked(new Map());
    anchorRef.current = null;
  }, [tab]);

  useEffect(() => {
    setTyped("");
  }, [confirmDel]);

  const goUp = useCallback(() => {
    if (crumbs.length > 1) setCur(crumbs[crumbs.length - 2].id);
  }, [crumbs]);

  // Atajos de teclado (el manejador siempre ve el estado más reciente).
  const keyRef = useRef<(e: KeyboardEvent) => void>(() => {});
  keyRef.current = (e: KeyboardEvent) => {
    const tag = (e.target as HTMLElement | null)?.tagName ?? "";
    const inField = ["INPUT", "SELECT", "TEXTAREA"].includes(tag);
    const ctrl = e.ctrlKey || e.metaKey;
    // F5 / Ctrl+R recargarían toda la ventana y se perdería el escaneo.
    if (e.key === "F5" || (ctrl && e.key.toLowerCase() === "r")) {
      e.preventDefault();
      if (phase === "done" && tab === "folders" && !confirmDel) refreshCur();
      return;
    }
    if (showManual) return; // el manual gestiona sus propias teclas (incluido F1)
    if (showAbout) {
      if (e.key === "Escape" || e.key === "Enter") setShowAbout(false);
      return;
    }
    if (e.key === "F1") {
      e.preventDefault();
      setShowManual(true);
      return;
    }
    if (showSettings) {
      if (e.key === "Escape") setShowSettings(false);
      return;
    }
    if (confirmDel) {
      if (e.key === "Escape" && !deleting) setConfirmDel(false);
      return;
    }
    if (phase !== "done") return;
    if (ctrl && e.key.toLowerCase() === "f") {
      e.preventDefault();
      setTab("search");
      setFocusSearch((n) => n + 1);
      return;
    }
    if (e.key === "Escape") {
      if (checked.size > 0) setChecked(new Map());
      return;
    }
    if (inField) return;
    const canSelect = tab === "folders" || tab === "search";
    if (e.key === "Delete" && canSelect && checked.size > 0) {
      e.preventDefault();
      setConfirmDel(true);
    } else if (ctrl && e.key.toLowerCase() === "a" && canSelect) {
      e.preventDefault();
      toggleAll(true);
    } else if (tab === "folders" && (e.key === "Backspace" || (e.altKey && e.key === "ArrowLeft"))) {
      e.preventDefault();
      goUp();
    }
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keyRef.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const start = async (override?: string) => {
    const p = (override ?? path).trim();
    if (override) setPath(override);
    if (!p || phase === "scanning") return;
    setError(null);
    setNotice(null);
    liveIds.current = new Map();
    setProgress(null);
    setDone(null);
    setPage(null);
    setTm(null);
    setCrumbs([]);
    setTopFiles(null);
    setExts(null);
    setErrs(null);
    setSres(null);
    setChecked(new Map());
    setTab("folders");
    setPhase("scanning");
    try {
      await api.startScan(p, threads);
      const next = [p, ...recent.filter((r) => r !== p)].slice(0, 8);
      setRecent(next);
      localStorage.setItem(LS_RECENT, JSON.stringify(next));
      localStorage.setItem(LS_LAST, p);
    } catch (e) {
      setError(String(e));
      setPhase("idle");
    }
  };

  const stop = () => {
    api.cancelScan().catch(() => {});
  };

  const browse = async () => {
    try {
      const sel = await open({ directory: true, multiple: false, defaultPath: path || undefined });
      if (typeof sel === "string") setPath(sel);
    } catch (e) {
      setError(String(e));
    }
  };

  const reveal = (id: number) => api.reveal(id).catch((e) => setError(String(e)));
  const copyPath = async (id: number) => {
    try {
      await navigator.clipboard.writeText(await api.path(id));
      flash("Ruta copiada");
    } catch (e) {
      setError(String(e));
    }
  };

  const changeThreads = (n: number) => {
    setThreads(n);
    localStorage.setItem(LS_THREADS, String(n));
  };

  const toggle = (p: Pick) =>
    setChecked((m) => {
      const n = new Map(m);
      if (n.has(p.id)) n.delete(p.id);
      else n.set(p.id, p);
      return n;
    });

  const selItems = useMemo(() => [...checked.values()], [checked]);
  const selSize = selItems.reduce((t, n) => t + n.size, 0);
  const selFolders = selItems.filter((n) => n.isDir).length;
  const visible: (NodeDto | SearchItem)[] = tab === "search" ? (sres?.items ?? []) : tab === "folders" ? (page?.items ?? []) : [];
  const allChecked = visible.length > 0 && visible.every((n) => checked.has(n.id));
  function toggleAll(forceOn = false) {
    setChecked(allChecked && !forceOn ? new Map() : new Map(visible.map((n) => [n.id, pickOf(n)])));
  }

  const rootPath = done?.root ?? "";
  const isNet =
    rootPath.startsWith("\\\\") ||
    drives.some((d) => d.kind === "red" && rootPath.toUpperCase().startsWith(d.path.toUpperCase()));
  const strongLimit = isNet ? STRONG_NET : STRONG_LOCAL;
  const strong = selSize >= strongLimit;
  const typedOk = typed.trim().toUpperCase() === "ELIMINAR";

  const onRowClick = (e: React.MouseEvent, n: NodeDto | SearchItem) => {
    const tgt = e.target as HTMLElement;
    const onCheck = tgt instanceof HTMLInputElement;
    if (!onCheck && tgt.closest("button, a, input, select")) return; // los enlaces y botones siguen a lo suyo
    if (e.shiftKey && anchorRef.current !== null) {
      const a = visible.findIndex((x) => x.id === anchorRef.current);
      const b = visible.findIndex((x) => x.id === n.id);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        setChecked((m) => {
          const nm = new Map(m);
          for (let i = lo; i <= hi; i++) nm.set(visible[i].id, pickOf(visible[i]));
          return nm;
        });
        window.getSelection()?.removeAllRanges();
        if (!onCheck) e.preventDefault(); // en una casilla, no: el navegador revertiría su estado visual
        return;
      }
    }
    if (e.ctrlKey || e.metaKey) {
      toggle(pickOf(n));
      anchorRef.current = n.id;
      if (!onCheck) e.preventDefault();
      return;
    }
    if (onCheck) {
      toggle(pickOf(n));
      anchorRef.current = n.id;
    }
  };

  const exportName = () => {
    const raw = tab === "search" ? "Busqueda" : (curNode?.name ?? "Escaneo");
    const last = raw.split(/[\\/]/).filter(Boolean).pop() ?? raw;
    return last.replace(/[:*?"<>|]/g, "").trim() || "Escaneo";
  };

  const doExport = async (kind: "folder" | "search", format: "pdf" | "csv") => {
    try {
      const stamp = new Date().toISOString().slice(0, 10);
      const sel = await save({
        defaultPath: `SGIIPP-DISKANALIZER - ${exportName()} - ${stamp}.${format}`,
        filters: [{ name: format === "pdf" ? "Informe PDF" : "CSV (Excel)", extensions: [format] }],
      });
      if (!sel) return;
      setExporting(true);
      const r = await api.exportReport(
        kind,
        kind === "folder" ? cur : null,
        kind === "search" ? searchArgs(50000) : null,
        format,
        sel,
        names.center.trim() || null,
        names.unit.trim() || null,
      );
      setNotice({
        text: `Informe guardado: ${fmtInt(r.rows)} elementos${r.truncated ? " (con un resumen del resto)" : ""}.`,
        path: r.path,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setExporting(false);
    }
  };

  const openExportMenu = (e: React.MouseEvent, kind: "folder" | "search") => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setExpMenu({ x: r.left, y: r.bottom + 4, kind });
  };

  useEffect(() => {
    const el = document.documentElement;
    if (theme === "auto") el.removeAttribute("data-theme");
    else el.setAttribute("data-theme", theme);
    try {
      localStorage.setItem(LS_THEME, theme);
    } catch {
      /* sin almacenamiento */
    }
  }, [theme]);

  const openSettings = () => {
    setDraft({ ...names });
    setShowSettings(true);
  };
  const saveSettings = () => {
    const next = { center: draft.center.trim(), unit: draft.unit.trim() };
    setNames(next);
    try {
      localStorage.setItem(LS_CENTER, next.center);
      localStorage.setItem(LS_UNIT, next.unit);
    } catch {
      /* sin almacenamiento: se usa solo en esta sesión */
    }
    setShowSettings(false);
    flash("Ajustes guardados");
  };

  const askDelete = (p: Pick) => {
    if (!checked.has(p.id)) setChecked(new Map([[p.id, p]]));
    setConfirmDel(true);
  };

  const ctxEntries = (p: Pick): MenuEntries => {
    const canSelect = tab === "folders" || tab === "search";
    const many = checked.has(p.id) && checked.size > 1;
    const list: (MenuItem | null | false)[] = [
      p.isDir && { label: "Abrir carpeta", hint: "clic", onClick: () => { setCur(p.id); setTab("folders"); } },
      !p.isDir && p.parent !== undefined && {
        label: "Ir a la carpeta que lo contiene",
        onClick: () => { setCur(p.parent as number); setTab("folders"); },
      },
      { label: "Mostrar en el Explorador", onClick: () => reveal(p.id) },
      { label: "Copiar ruta", onClick: () => copyPath(p.id) },
      null,
      canSelect && { label: checked.has(p.id) ? "Quitar de la selección" : "Seleccionar", onClick: () => toggle(p) },
      {
        label: many ? `Eliminar los ${fmtInt(checked.size)} seleccionados…` : "Eliminar…",
        hint: "Supr",
        danger: true,
        onClick: () => askDelete(p),
      },
    ];
    return list.filter((x): x is MenuItem | null => x !== false);
  };

  const openCtx = (e: React.MouseEvent, p: Pick) => {
    e.preventDefault();
    setCtx({ x: e.clientX, y: e.clientY, p });
  };

  const confirmDelete = async () => {
    setDeleting(true);
    setDelProgress(null);
    try {
      await api.startDelete([...checked.keys()]);
    } catch (e) {
      setError(String(e));
      setDeleting(false);
      setConfirmDel(false);
    }
  };

  const stopDelete = () => {
    api.cancelDelete().catch(() => {});
  };

  const refreshCur = async () => {
    if (refreshing || deleting) return;
    const parent = crumbs.length > 1 ? crumbs[crumbs.length - 2].id : 0;
    setRefreshing(true);
    try {
      const r = await api.refresh(cur);
      setDone((d) => (d ? { ...d, size: r.size, files: r.files, dirs: r.dirs } : d));
      setTopFiles(null);
      setExts(null);
      setChecked(new Map());
      if (r.removed) setCur(parent);
      setReload((n) => n + 1);
      flash(r.removed ? "La carpeta ya no existe" : "Carpeta actualizada");
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  };

  const selBar =
    checked.size > 0 ? (
      <div className="selbar">
        <span>
          <b>{fmtInt(checked.size)}</b> seleccionados · hasta {fmtBytes(selSize)}
        </span>
        <button onClick={() => setChecked(new Map())}>Quitar selección</button>
        <button className="danger solid" onClick={() => setConfirmDel(true)}>
          <Icon.trash /> Eliminar… <kbd>Supr</kbd>
        </button>
      </div>
    ) : null;

  const basePath = crumbs.reduce((p, c, i) => (i === 0 ? c.name : joinPath(p, c.name)), "");

  const running = phase === "scanning";

  // Entradas de primer nivel del escaneo en curso, con ids estables por nombre (para que el treemap las anime bien).
  const liveItems: NodeDto[] = useMemo(() => {
    if (!progress) return [];
    const ids = liveIds.current;
    return progress.top.map((t) => {
      let id = ids.get(t.name);
      if (id === undefined) {
        id = ids.size + 1;
        ids.set(t.name, id);
      }
      return { id, parent: 0, name: t.name, isDir: t.isDir, size: t.size, files: t.files, mtime: 0 };
    });
  }, [progress]);
  const liveTotal = progress?.bytes ?? 0;
  const curNode = crumbs[crumbs.length - 1];
  const curSize = curNode?.size ?? 0;

  const sortTh = (key: SortKey, label: string, cls = "") => (
    <th
      className={cls + " sortable"}
      onClick={() => setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "name" }))}
    >
      {label}
      {sort.key === key && <span className="arrow">{sort.desc ? "▼" : "▲"}</span>}
    </th>
  );

  const rowActions = (id: number) => (
    <td className="act">
      <button title="Mostrar en el Explorador" onClick={() => reveal(id)}>
        <Icon.reveal />
      </button>
      <button title="Copiar ruta" onClick={() => copyPath(id)}>
        <Icon.copy />
      </button>
    </td>
  );

  return (
    <div className="app">
      {/* ───── Barra superior ───── */}
      <header className="toolbar">
        <div className="row">
          <input
            className="path"
            list="recientes"
            value={path}
            placeholder={"\\\\servidor\\recurso   ·   Z:\\   ·   C:\\Datos"}
            spellCheck={false}
            disabled={running}
            onChange={(e) => setPath(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && start()}
          />
          <datalist id="recientes">
            {recent.map((r) => (
              <option key={r} value={r} />
            ))}
          </datalist>
          <button disabled={running} onClick={browse}>
            Examinar…
          </button>
          <label className="threads" title="Más hilos = más rápido en red; en un disco mecánico local conviene menos">
            Hilos
            <select value={threads} disabled={running} onChange={(e) => changeThreads(Number(e.target.value))}>
              {[4, 8, 16, 32, 64].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          {running ? (
            <button className="danger" onClick={stop}>
              Detener
            </button>
          ) : (
            <button className="primary" disabled={!path.trim()} onClick={() => start()}>
              Escanear
            </button>
          )}
        </div>
        <div className="row drives">
          {drives.map((d) => (
            <button key={d.path} className="chip" disabled={running} onClick={() => setPath(d.path)}>
              <Icon.drive />
              {d.path}
              {DRIVE_KIND[d.kind] && <small>{DRIVE_KIND[d.kind]}</small>}
            </button>
          ))}
          <span className="tools">
            <button className="tool" onClick={() => setTheme(THEME_NEXT[theme])} title="Cambiar entre tema automático, claro y oscuro">
              {theme === "dark" ? <Icon.moon /> : theme === "light" ? <Icon.sun /> : <Icon.half />} {THEME_LABEL[theme]}
            </button>
            <button className="tool" onClick={openSettings} title="Nombre del centro para los informes">
              <Icon.gear /> Ajustes
            </button>
            <button className="tool" onClick={() => setShowAbout(true)} title="Créditos y versión">
              <Icon.help /> Acerca de
            </button>
            <button className="tool" onClick={() => setShowManual(true)} title="Abrir el manual (F1)">
              <Icon.help /> Manual <kbd>F1</kbd>
            </button>
          </span>
        </div>
      </header>

      {error && (
        <div className="banner">
          <span>{error}</span>
          <button onClick={() => setError(null)} aria-label="Cerrar">
            ×
          </button>
        </div>
      )}

      {notice && (
        <div className="banner info">
          <span>{notice.text}</span>
          <button className="link-btn" onClick={() => api.revealPath(notice.path).catch((e) => setError(String(e)))}>
            Mostrar en el Explorador
          </button>
          <button onClick={() => setNotice(null)} aria-label="Cerrar">
            ×
          </button>
        </div>
      )}

      {/* ───── Inicio ───── */}
      {phase === "idle" && (
        <div className="home">
          <div className="hero">
            <h1>SGIIPP-DISKANALIZER</h1>
            <p>
              Escribe una ruta de red (<code>\\servidor\recurso</code>), elige una unidad o pulsa «Examinar…».
              Se usan las credenciales de tu sesión de Windows.
            </p>
          </div>
          {drives.length > 0 && (
            <section aria-labelledby="home-drives">
              <h2 id="home-drives">Unidades</h2>
              <div className="cards">
                {drives.map((d) => {
                  const sp = spaces[d.path];
                  const used = sp ? sp.total - sp.free : 0;
                  const p = sp ? pct(used, sp.total) : 0;
                  return (
                    <button key={d.path} className="card" onClick={() => start(d.path)} title={`Escanear ${d.path}`}>
                      <span className="card-top">
                        <Icon.drive />
                        <b>{d.path}</b>
                        {DRIVE_KIND[d.kind] && <small>{DRIVE_KIND[d.kind]}</small>}
                      </span>
                      <span className="pbar big" aria-hidden="true">
                        <i className={barTone(p >= 90 ? 40 : p >= 75 ? 15 : 0)} style={{ width: `${p}%` }} />
                      </span>
                      <span className="card-info">
                        {sp ? `${fmtBytes(sp.free)} libres de ${fmtBytes(sp.total)} · ${fmtPct(p)} usado` : spaces[d.path] === undefined ? "Consultando…" : "Capacidad no disponible"}
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>
          )}
          {recent.length > 0 && (
            <section aria-labelledby="home-recent">
              <h2 id="home-recent">Últimas rutas</h2>
              <ul className="recent-list">
                {recent.map((r) => (
                  <li key={r}>
                    <button className="recent" onClick={() => start(r)} title={`Escanear ${r}`}>
                      <Icon.folder /> {r}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      )}

      {/* ───── Escaneando (con vista en vivo) ───── */}
      {running && (
        <div className="live">
          <div className="livehead">
            <div className="counters compact">
              <Counter label="Archivos" value={fmtInt(progress?.files ?? 0)} />
              <Counter label="Carpetas" value={fmtInt(progress?.dirs ?? 0)} />
              <Counter label="Tamaño encontrado" value={fmtBytes(progress?.bytes ?? 0)} />
              <Counter label="Tiempo" value={fmtDur(progress?.elapsedMs ?? 0)} />
              <Counter
                label="Velocidad"
                value={
                  progress && progress.elapsedMs > 500
                    ? `${fmtInt(Math.round((progress.files / progress.elapsedMs) * 1000))} arch/s`
                    : "—"
                }
              />
              {(progress?.errors ?? 0) > 0 && <Counter label="Sin acceso" value={fmtInt(progress?.errors ?? 0)} warn />}
            </div>
            <button onClick={stop}>Detener y ver lo escaneado</button>
          </div>
          <div className="indeterminate" />
          <div className="current" title={progress?.current}>
            {shortenPath(progress?.current ?? "Preparando…", 120)}
          </div>
          <div className="split">
            <section className="panel">
              <div className="note info">Vista en vivo del primer nivel: las cifras siguen creciendo hasta terminar.</div>
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th className="c-name">Nombre</th>
                      <th className="c-size">Tamaño hasta ahora</th>
                      <th className="num">%</th>
                      <th className="num">Archivos</th>
                    </tr>
                  </thead>
                  <tbody>
                    {liveItems.map((n) => {
                      const p = pct(n.size, liveTotal);
                      return (
                        <tr key={n.id}>
                          <td className="c-name">
                            <div className="nmwrap">
                              <span className={"ic" + (n.isDir ? " dir" : "")}>
                                {n.isDir ? <Icon.folder /> : <Icon.file />}
                              </span>
                              <span className="nm" title={n.name}>
                                {n.name}
                              </span>
                            </div>
                          </td>
                          <td className="c-size">
                            <div className="bar" style={{ width: `${Math.min(100, p)}%` }} />
                            <span>{fmtBytes(n.size)}</span>
                          </td>
                          <td className="num">{fmtPct(p)}</td>
                          <td className="num">{n.isDir ? fmtInt(n.files) : ""}</td>
                        </tr>
                      );
                    })}
                    {progress && progress.topTotal > liveItems.length && (
                      <tr className="rest-row">
                        <td colSpan={4}>
                          … y {fmtInt(progress.topTotal - liveItems.length)} elementos más ({fmtBytes(progress.topRestSize)})
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
                {!progress && <div className="empty">Preparando…</div>}
              </div>
            </section>
            <section className="panel map">
              <Treemap live items={liveItems} restSize={progress?.topRestSize ?? 0} restCount={(progress?.topTotal ?? 0) - liveItems.length} onOpen={() => {}} />
            </section>
          </div>
        </div>
      )}

      {/* ───── Resultados ───── */}
      {phase === "done" && done && (
        <>
          <div className="summary">
            <div className="total">
              <span className="big">{fmtBytes(done.size)}</span>
              <span className="root" title={done.root}>
                {shortenPath(done.root, 60)}
              </span>
            </div>
            <Stat label="archivos" value={fmtInt(done.files)} />
            <Stat label="carpetas" value={fmtInt(done.dirs)} />
            <Stat label="duración" value={fmtDur(done.elapsedMs)} />
            {done.errors > 0 && (
              <button className="stat warn" onClick={() => setTab("errors")}>
                <b>{fmtInt(done.errors)}</b> sin acceso
              </button>
            )}
            {done.cancelled && <span className="tag">Escaneo parcial</span>}
          </div>

          <nav className="tabs">
            {(
              [
                ["folders", "Carpetas"],
                ["search", "Buscar"],
                ["files", "Archivos más grandes"],
                ["ext", "Por extensión"],
                ["errors", `Errores${done.errors ? ` (${fmtInt(done.errors)})` : ""}`],
              ] as [Tab, string][]
            ).map(([k, label]) => (
              <button key={k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
                {label}
              </button>
            ))}
          </nav>

          {tab === "folders" && (
            <div className="split">
              <section className="panel">
                <div className="crumbs">
                  <button className="icon" title="Subir (Retroceso)" disabled={crumbs.length < 2} onClick={goUp}>
                    <Icon.up />
                  </button>
                  <button
                    className="icon"
                    title="Actualizar esta carpeta desde el disco"
                    disabled={refreshing || deleting}
                    onClick={refreshCur}
                  >
                    <span className={refreshing ? "spin" : ""}>
                      <Icon.refresh />
                    </span>
                  </button>
                  {crumbs.map((c, i) => (
                    <span key={c.id} className="crumb">
                      {i > 0 && <em>›</em>}
                      <button
                        className={i === crumbs.length - 1 ? "here" : ""}
                        onClick={() => setCur(c.id)}
                        title={c.name}
                      >
                        {i === 0 ? shortenPath(c.name, 40) : c.name}
                      </button>
                    </span>
                  ))}
                  <button
                    className="export"
                    title="Guardar un informe de esta carpeta (PDF o CSV para Excel)"
                    disabled={exporting}
                    onClick={(e) => openExportMenu(e, "folder")}
                  >
                    <Icon.export /> Exportar…
                  </button>
                </div>
                {selBar}
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th className="chk">
                          <input
                            type="checkbox"
                            checked={allChecked}
                            onChange={() => toggleAll()}
                            title="Seleccionar todo lo visible"
                          />
                        </th>
                        {sortTh("name", "Nombre", "c-name")}
                        {sortTh("size", "Tamaño", "c-size")}
                        <th className="c-pct">%</th>
                        {sortTh("files", "Archivos", "num")}
                        {sortTh("mtime", "Modificado", "date")}
                        <th className="act" />
                      </tr>
                    </thead>
                    <tbody>
                      {page?.items.map((n) => {
                        const p = pct(n.size, curSize);
                        return (
                          <tr
                            key={n.id}
                            className={(checked.has(n.id) ? "sel" : "") + (hoverId === n.id ? " hl" : "")}
                            onClick={(e) => onRowClick(e, n)}
                            onMouseEnter={() => setHoverId(n.id)}
                            onMouseLeave={() => setHoverId(null)}
                            onMouseDown={(e) => e.shiftKey && e.preventDefault()}
                            onContextMenu={(e) => openCtx(e, pickOf(n))}
                          >
                            <td className="chk">
                              <input type="checkbox" checked={checked.has(n.id)} onChange={noop} />
                            </td>
                            <td className="c-name">
                              <div className="nmwrap">
                                <span className={"ic" + (n.isDir ? " dir" : "")}>
                                  {n.isDir ? <Icon.folder /> : <Icon.file />}
                                </span>
                                {n.isDir ? (
                                  <button className="nm link" title={n.name} onClick={() => setCur(n.id)}>
                                    {n.name}
                                  </button>
                                ) : (
                                  <span className="nm" title={n.name}>
                                    {n.name}
                                  </span>
                                )}
                              </div>
                            </td>
                            <td className="c-size">
                              <span>{fmtBytes(n.size)}</span>
                            </td>
                            <td className="c-pct">
                              <div className="pbar" aria-hidden="true">
                                <i className={barTone(p)} style={{ width: `${Math.max(p > 0 ? 2 : 0, Math.min(100, p))}%` }} />
                              </div>
                              <span>{fmtPct(p)}</span>
                            </td>
                            <td className="num">{n.isDir ? fmtInt(n.files) : ""}</td>
                            <td className="date">{fmtDate(n.mtime)}</td>
                            {rowActions(n.id)}
                          </tr>
                        );
                      })}
                      {page && page.restCount > 0 && (
                        <tr className="rest-row">
                          <td colSpan={7}>
                            … y {fmtInt(page.restCount)} elementos más ({fmtBytes(page.restSize)})
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                  {page && page.total === 0 && <div className="empty">Esta carpeta está vacía</div>}
                </div>
              </section>
              <section className="panel map">
                {tm && (
                  <Treemap
                    items={tm.items}
                    restSize={tm.restSize}
                    restCount={tm.restCount}
                    basePath={basePath}
                    highlightId={hoverId}
                    onHover={setHoverId}
                    onOpen={(n) => setCur(n.id)}
                    onContext={(n, x, y) => setCtx({ x, y, p: pickOf(n) })}
                  />
                )}
              </section>
            </div>
          )}

          {tab === "search" && (
            <section className="panel full">
              <div className="filters">
                <label className="f-text">
                  <Icon.search />
                  <input
                    ref={searchRef}
                    value={sq.text}
                    placeholder="Nombre: backup · *.bak · informe?  (Ctrl+F)"
                    spellCheck={false}
                    onChange={(e) => setSq({ ...sq, text: e.target.value })}
                  />
                </label>
                <label>
                  Tipo
                  <select value={sq.kind} onChange={(e) => setSq({ ...sq, kind: e.target.value as SearchArgs["kind"] })}>
                    <option value="files">Archivos</option>
                    <option value="dirs">Carpetas</option>
                    <option value="all">Ambos</option>
                  </select>
                </label>
                <label>
                  Tamaño mínimo
                  <span className="pair">
                    <input
                      className="narrow"
                      inputMode="decimal"
                      placeholder="0"
                      value={sq.size}
                      onChange={(e) => setSq({ ...sq, size: e.target.value })}
                    />
                    <select value={sq.unit} onChange={(e) => setSq({ ...sq, unit: e.target.value })}>
                      {Object.keys(UNITS).map((u) => (
                        <option key={u}>{u}</option>
                      ))}
                    </select>
                  </span>
                </label>
                <label title="Sin modificar desde hace al menos…">
                  Sin tocar
                  <select value={sq.age} onChange={(e) => setSq({ ...sq, age: e.target.value })}>
                    {AGES.map(([v, l]) => (
                      <option key={v} value={v}>
                        {l}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Extensiones
                  <input
                    className="exts"
                    placeholder="iso, zip, bak"
                    spellCheck={false}
                    value={sq.exts}
                    onChange={(e) => setSq({ ...sq, exts: e.target.value })}
                  />
                </label>
                <button onClick={() => setSq(EMPTY_SEARCH)} disabled={JSON.stringify(sq) === JSON.stringify(EMPTY_SEARCH)}>
                  Limpiar
                </button>
              </div>
              <div className="searchsum">
                {sres ? (
                  <>
                    <b>{fmtInt(sres.count)}</b> {sres.count === 1 ? "resultado" : "resultados"}
                    {sres.totalSize !== null && <> · {fmtBytes(sres.totalSize)} en total</>}
                    {sres.count > sres.items.length && <> · se muestran los {fmtInt(sres.items.length)} más grandes</>}
                  </>
                ) : (
                  "Buscando…"
                )}
                {searching && sres && <span className="muted"> · actualizando…</span>}
                <button
                  className="export"
                  title="Guardar un informe de estos resultados (PDF o CSV para Excel)"
                  disabled={exporting || !sres || sres.count === 0}
                  onClick={(e) => openExportMenu(e, "search")}
                >
                  <Icon.export /> Exportar…
                </button>
              </div>
              {selBar}
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th className="chk">
                        <input
                          type="checkbox"
                          checked={allChecked}
                          onChange={() => toggleAll()}
                          title="Seleccionar todo lo visible (Ctrl+A)"
                        />
                      </th>
                      <th className="c-name">Nombre</th>
                      <th className="c-size">Tamaño</th>
                      <th className="num">Archivos</th>
                      <th className="date">Modificado</th>
                      <th className="act" />
                    </tr>
                  </thead>
                  <tbody>
                    {sres?.items.map((n) => (
                      <tr
                        key={n.id}
                        className={checked.has(n.id) ? "sel" : ""}
                        onClick={(e) => onRowClick(e, n)}
                        onMouseDown={(e) => e.shiftKey && e.preventDefault()}
                        onContextMenu={(e) => openCtx(e, pickOf(n))}
                      >
                        <td className="chk">
                          <input type="checkbox" checked={checked.has(n.id)} onChange={noop} />
                        </td>
                        <td className="c-name two">
                          <div className="nmwrap">
                            <span className={"ic" + (n.isDir ? " dir" : "")}>{n.isDir ? <Icon.folder /> : <Icon.file />}</span>
                            <span className="stack">
                              <span className="nm" title={n.name}>
                                {n.name}
                              </span>
                              <button
                                className="sub link"
                                title="Ir a la carpeta que lo contiene"
                                onClick={() => {
                                  setCur(n.parent);
                                  setTab("folders");
                                }}
                              >
                                {shortenPath(dirOf(n), 110)}
                              </button>
                            </span>
                          </div>
                        </td>
                        <td className="c-size">
                          <div className="bar" style={{ width: `${Math.min(100, pct(n.size, sres.items[0]?.size ?? 0))}%` }} />
                          <span>{fmtBytes(n.size)}</span>
                        </td>
                        <td className="num">{n.isDir ? fmtInt(n.files) : ""}</td>
                        <td className="date">{fmtDate(n.mtime)}</td>
                        {rowActions(n.id)}
                      </tr>
                    ))}
                  </tbody>
                </table>
                {sres && sres.items.length === 0 && <div className="empty">Nada coincide con esos filtros</div>}
              </div>
            </section>
          )}

          {tab === "files" && (
            <section className="panel full">
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th className="c-name">Archivo</th>
                      <th className="c-size">Tamaño</th>
                      <th className="date">Modificado</th>
                      <th className="act" />
                    </tr>
                  </thead>
                  <tbody>
                    {topFiles?.map((f) => (
                      <tr
                        key={f.id}
                        onContextMenu={(e) => openCtx(e, { id: f.id, name: f.name, isDir: false, size: f.size, parent: f.parent })}
                      >
                        <td className="c-name two">
                          <div className="nmwrap">
                            <span className="ic">
                              <Icon.file />
                            </span>
                            <span className="stack">
                              <span className="nm" title={f.name}>
                                {f.name}
                              </span>
                              <button
                                className="sub link"
                                title="Ir a esta carpeta"
                                onClick={() => {
                                  setCur(f.parent);
                                  setTab("folders");
                                }}
                              >
                                {shortenPath(dirOf(f), 110)}
                              </button>
                            </span>
                          </div>
                        </td>
                        <td className="c-size">
                          <div
                            className="bar"
                            style={{ width: `${Math.min(100, pct(f.size, topFiles[0]?.size ?? 0))}%` }}
                          />
                          <span>{fmtBytes(f.size)}</span>
                        </td>
                        <td className="date">{fmtDate(f.mtime)}</td>
                        {rowActions(f.id)}
                      </tr>
                    ))}
                  </tbody>
                </table>
                {!topFiles && <div className="empty">Calculando…</div>}
              </div>
            </section>
          )}

          {tab === "ext" && (
            <section className="panel full">
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th className="c-name">Extensión</th>
                      <th className="c-size">Tamaño</th>
                      <th className="num">% del total</th>
                      <th className="num">Archivos</th>
                    </tr>
                  </thead>
                  <tbody>
                    {exts?.map((x) => {
                      const p = pct(x.size, done.size);
                      const rel = pct(x.size, exts[0]?.size ?? 0);
                      const cat = categoryOf(`x.${x.ext}`, false);
                      return (
                        <tr key={x.ext || "(sin)"}>
                          <td className="c-name">
                            <div className="nmwrap">
                              <span className="dot" style={{ background: CATEGORY_INFO[cat].color }} />
                              <span className="nm">{x.ext ? `.${x.ext}` : "(sin extensión)"}</span>
                            </div>
                          </td>
                          <td className="c-size">
                            <div className="bar" style={{ width: `${Math.min(100, rel)}%` }} />
                            <span>{fmtBytes(x.size)}</span>
                          </td>
                          <td className="num">{fmtPct(p)}</td>
                          <td className="num">{fmtInt(x.count)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
                {!exts && <div className="empty">Calculando…</div>}
              </div>
            </section>
          )}

          {tab === "errors" && (
            <section className="panel full">
              <div className="table-scroll">
                {errs && done.errors > errs.length && (
                  <div className="note">
                    Se muestran los primeros {fmtInt(errs.length)} de {fmtInt(done.errors)} errores.
                  </div>
                )}
                <table>
                  <thead>
                    <tr>
                      <th className="c-name">Ruta</th>
                      <th>Motivo</th>
                    </tr>
                  </thead>
                  <tbody>
                    {errs?.map((e, i) => (
                      <tr key={i}>
                        <td className="c-name">
                          <span className="nm" title={e.path}>
                            {e.path}
                          </span>
                        </td>
                        <td className="reason">{e.message}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {errs && errs.length === 0 && <div className="empty">Sin errores: se pudo leer todo</div>}
                {!errs && <div className="empty">Cargando…</div>}
              </div>
            </section>
          )}
        </>
      )}

      {confirmDel && (
        <div className="modal-back" onClick={() => !deleting && setConfirmDel(false)}>
          <div className="modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
            {!deleting ? (
              <>
                <h2>Eliminar definitivamente</h2>
                <p>
                  Vas a eliminar <b>{fmtInt(selItems.length)}</b>{" "}
                  {selFolders > 0
                    ? `elemento${selItems.length === 1 ? "" : "s"} (${fmtInt(selFolders)} carpeta${selFolders === 1 ? "" : "s"} con todo su contenido)`
                    : `archivo${selItems.length === 1 ? "" : "s"}`}
                  , <b>{fmtBytes(selSize)}</b> en total.
                </p>
                <ul>
                  {selItems.slice(0, 8).map((n) => (
                    <li key={n.id}>
                      <span className={"ic" + (n.isDir ? " dir" : "")}>
                        {n.isDir ? <Icon.folder /> : <Icon.file />}
                      </span>
                      <span className="nm">{n.name}</span>
                      <span className="sz">{fmtBytes(n.size)}</span>
                    </li>
                  ))}
                  {selItems.length > 8 && <li className="more">… y {fmtInt(selItems.length - 8)} más</li>}
                </ul>
                <p className="warn-text">
                  No va a la papelera: <b>no se puede deshacer</b>.
                </p>
                {strong && (
                  <div className="strong">
                    <p>
                      Son <b>{fmtBytes(selSize)}</b>
                      {isNet ? " en una unidad de red" : ""}. Para confirmar, escribe <b>ELIMINAR</b>:
                    </p>
                    <input
                      autoFocus
                      className="typed"
                      value={typed}
                      placeholder="ELIMINAR"
                      spellCheck={false}
                      onChange={(e) => setTyped(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && typedOk && confirmDelete()}
                    />
                  </div>
                )}
                <div className="modal-actions">
                  <button autoFocus={!strong} onClick={() => setConfirmDel(false)}>
                    Cancelar
                  </button>
                  <button className="danger solid" disabled={strong && !typedOk} onClick={confirmDelete}>
                    Eliminar definitivamente
                  </button>
                </div>
              </>
            ) : (
              <>
                <h2>{delProgress?.phase === "refresh" ? "Actualizando lo que queda…" : "Eliminando…"}</h2>
                <div className="indeterminate" />
                <div className="counters small">
                  <Counter label="Archivos eliminados" value={fmtInt(delProgress?.files ?? 0)} />
                  <Counter label="Liberado" value={fmtBytes(delProgress?.bytes ?? 0)} />
                  <Counter label="Tiempo" value={fmtDur(delProgress?.elapsedMs ?? 0)} />
                  {(delProgress?.errors ?? 0) > 0 && (
                    <Counter label="Con error" value={fmtInt(delProgress?.errors ?? 0)} warn />
                  )}
                </div>
                {delProgress && delProgress.total > 1 && (
                  <p className="muted">
                    Elemento {delProgress.index} de {delProgress.total}: {delProgress.name}
                  </p>
                )}
                <div className="current" title={delProgress?.current}>
                  {shortenPath(delProgress?.current ?? "Preparando…", 70)}
                </div>
                <div className="modal-actions">
                  <button onClick={stopDelete}>Detener</button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {showManual && <Manual onClose={() => setShowManual(false)} />}

      {showAbout && (
        <div className="modal-back" onMouseDown={() => setShowAbout(false)}>
          <div
            className="modal about"
            role="dialog"
            aria-modal="true"
            aria-labelledby="about-title"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <div className="about-banner">
              <h2 id="about-title">SGIIPP-DISKANALIZER</h2>
              <span>Analizador de espacio en disco · v{__APP_VERSION__}</span>
            </div>
            <dl className="credits">
              <dt>Desarrollado por</dt>
              <dd>Servicios de Informática</dd>
              <dt>Centro</dt>
              <dd>
                Centro de Inserción Social «Guillermo Miranda»
                <br />
                Murcia
              </dd>
              <dt>Tecnología</dt>
              <dd>Rust · Tauri 2 · React</dd>
            </dl>
            <div className="donate">
              <img src={donateQr} alt={`Código QR para donar mediante PayPal (${DONATE_URL})`} />
              <div>
                <h3>Apoya el proyecto</h3>
                <p className="muted small">
                  Si la aplicación te resulta útil, puedes hacer una donación voluntaria.
                </p>
                <div className="donate-actions">
                  <button className="primary" onClick={() => api.openDonate().catch((e) => setError(String(e)))}>
                    Donar con PayPal
                  </button>
                  <button
                    onClick={() => {
                      navigator.clipboard?.writeText(DONATE_URL).then(
                        () => flash("Enlace copiado"),
                        () => {},
                      );
                    }}
                  >
                    Copiar enlace
                  </button>
                </div>
              </div>
            </div>
            <p className="muted small">© {new Date().getFullYear()} Servicios de Informática · CIS Guillermo Miranda · Murcia.</p>
            <div className="modal-actions">
              <button className="primary" autoFocus onClick={() => setShowAbout(false)}>
                Cerrar
              </button>
            </div>
          </div>
        </div>
      )}

      {showSettings && (
        <div className="modal-back" onMouseDown={() => setShowSettings(false)}>
          <div
            className="modal settings"
            role="dialog"
            aria-modal="true"
            aria-labelledby="set-title"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <h2 id="set-title">Ajustes de los informes</h2>
            <p className="muted">
              Estos nombres salen en la banda, la cabecera y el pie de los informes PDF. Si dejas un campo vacío se usa
              el valor por defecto.
            </p>
            <label htmlFor="set-center">Nombre del centro</label>
            <input
              id="set-center"
              autoFocus
              maxLength={150}
              value={draft.center}
              placeholder={defaults.center}
              onChange={(e) => setDraft({ ...draft, center: e.target.value })}
              onKeyDown={(e) => e.key === "Enter" && saveSettings()}
            />
            <label htmlFor="set-unit">Servicio o unidad</label>
            <input
              id="set-unit"
              maxLength={150}
              value={draft.unit}
              placeholder={defaults.unit}
              onChange={(e) => setDraft({ ...draft, unit: e.target.value })}
              onKeyDown={(e) => e.key === "Enter" && saveSettings()}
            />
            {defaults.fromFile && (
              <p className="muted small">
                Los valores por defecto proceden del archivo <code>espacio.ini</code> junto al programa.
              </p>
            )}
            <div className="preview" aria-label="Vista previa de la banda del informe">
              <span className="pv-label">Vista previa</span>
              <b>{(draft.center.trim() || defaults.center).toUpperCase()}</b>
              <i>{draft.unit.trim() || defaults.unit}</i>
            </div>
            <div className="modal-actions">
              <button onClick={() => setDraft({ center: "", unit: "" })}>Restablecer</button>
              <button onClick={() => setShowSettings(false)}>Cancelar</button>
              <button className="primary" onClick={saveSettings}>
                Guardar
              </button>
            </div>
          </div>
        </div>
      )}

      {expMenu && (
        <ContextMenu
          x={expMenu.x}
          y={expMenu.y}
          title="Exportar informe"
          entries={[
            { label: "Informe PDF", onClick: () => doExport(expMenu.kind, "pdf") },
            { label: "Archivo CSV (para Excel)", onClick: () => doExport(expMenu.kind, "csv") },
          ]}
          onClose={() => setExpMenu(null)}
        />
      )}

      {ctx && (
        <ContextMenu x={ctx.x} y={ctx.y} title={ctx.p.name} entries={ctxEntries(ctx.p)} onClose={() => setCtx(null)} />
      )}

      <footer className="credits">
        <span>
          <button className="link-btn" onClick={() => setShowAbout(true)} title="Créditos y versión">
            SGIIPP-DISKANALIZER v{__APP_VERSION__}
          </button>{" "}
          · Servicios de Informática · CIS Guillermo Miranda · Murcia
        </span>
        {phase === "done" && (
          <span className="keys">
            <kbd>F1</kbd> manual · <kbd>F5</kbd> actualizar · <kbd>Ctrl</kbd>+<kbd>F</kbd> buscar · <kbd>Supr</kbd> eliminar · <kbd>Shift</kbd>/<kbd>Ctrl</kbd>+clic: varios · clic derecho: más opciones
          </span>
        )}
      </footer>

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function Counter({ label, value, warn }: { label: string; value: string; warn?: boolean }) {
  return (
    <div className={"counter" + (warn ? " warn" : "")}>
      <b>{value}</b>
      <span>{label}</span>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <b>{value}</b> {label}
    </div>
  );
}
