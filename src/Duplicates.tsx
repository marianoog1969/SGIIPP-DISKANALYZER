import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { Icon } from "./Icons";
import { fmtBytes, fmtDate, fmtInt, pct } from "./format";
import type { DupDone, DupGroup, DupProgress, Pick } from "./types";

interface Props {
  /** Hilos de lectura (los mismos que el escaneo). */
  threads: number;
  /** Cambia cada vez que se borra algo: se comprueba qué copias siguen existiendo. */
  reload: number;
  /** Pide eliminar estas copias (la aplicación enseña su confirmación de siempre). */
  onDelete: (picks: Pick[]) => void;
  onError: (msg: string) => void;
}

const MIN_OPTIONS: [string, number][] = [
  ["100 KB", 100 * 1024],
  ["1 MB", 1024 ** 2],
  ["10 MB", 10 * 1024 ** 2],
  ["100 MB", 100 * 1024 ** 2],
];

const PHASE: Record<DupProgress["phase"], string> = {
  tamaños: "Agrupando por tamaño…",
  inicio: "Comparando el principio de los archivos…",
  contenido: "Comparando el contenido completo…",
};

export default function Duplicates({ threads, reload, onDelete, onError }: Props) {
  const [minIdx, setMinIdx] = useState(1);
  const [running, setRunning] = useState(false);
  const [prog, setProg] = useState<DupProgress | null>(null);
  const [res, setRes] = useState<DupDone | null>(null);
  const [groups, setGroups] = useState<DupGroup[]>([]);
  const [sel, setSel] = useState<Set<number>>(new Set());
  const [open, setOpen] = useState<Set<number>>(new Set());
  const pending = useRef<number[]>([]);

  useEffect(() => {
    const subs = [
      listen<DupProgress>("dup-progress", (e) => setProg(e.payload)),
      listen<DupDone>("dup-done", (e) => {
        setRes(e.payload);
        setGroups(e.payload.groups);
        setSel(new Set());
        setOpen(new Set(e.payload.groups.slice(0, 3).map((_, i) => i)));
        setRunning(false);
        setProg(null);
      }),
      listen<string>("dup-error", (e) => {
        onError(e.payload);
        setRunning(false);
        setProg(null);
      }),
    ];
    return () => {
      subs.forEach((p) => p.then((off) => off()));
    };
  }, [onError]);

  // Tras un borrado: quitar de la lista lo que ya no existe.
  useEffect(() => {
    const ids = pending.current;
    if (ids.length === 0) return;
    pending.current = [];
    Promise.allSettled(ids.map((id) => api.ancestors(id).then(() => id))).then((rs) => {
      const gone = new Set<number>();
      rs.forEach((r, i) => r.status === "rejected" && gone.add(ids[i]));
      if (gone.size === 0) return;
      setGroups((gs) => gs.map((g) => ({ ...g, files: g.files.filter((f) => !gone.has(f.id)) })).filter((g) => g.files.length > 1));
      setSel(new Set());
    });
  }, [reload]);

  const start = useCallback(() => {
    setRes(null);
    setGroups([]);
    setSel(new Set());
    setProg(null);
    setRunning(true);
    api.findDuplicates(MIN_OPTIONS[minIdx][1], threads).catch((e) => {
      onError(String(e));
      setRunning(false);
    });
  }, [minIdx, threads, onError]);

  const wasted = useMemo(() => groups.reduce((s, g) => s + g.size * (g.files.length - 1), 0), [groups]);
  const selSize = useMemo(() => {
    let s = 0;
    for (const g of groups) for (const f of g.files) if (sel.has(f.id)) s += g.size;
    return s;
  }, [groups, sel]);
  // Nunca se deja marcar todas las copias de un grupo: siempre queda una.
  const allMarked = useMemo(
    () => groups.filter((g) => g.files.every((f) => sel.has(f.id))).length,
    [groups, sel],
  );

  const toggle = (id: number) =>
    setSel((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  /** Marca todas las copias menos la más antigua (la primera). */
  const markGroup = (g: DupGroup, on: boolean) =>
    setSel((s) => {
      const n = new Set(s);
      g.files.forEach((f, i) => (i === 0 || !on ? n.delete(f.id) : n.add(f.id)));
      return n;
    });
  const markAll = () =>
    setSel(() => {
      const n = new Set<number>();
      groups.forEach((g) => g.files.slice(1).forEach((f) => n.add(f.id)));
      return n;
    });

  const doDelete = () => {
    const picks: Pick[] = [];
    for (const g of groups)
      for (const f of g.files) if (sel.has(f.id)) picks.push({ id: f.id, name: f.name, isDir: false, size: g.size });
    pending.current = picks.map((p) => p.id);
    onDelete(picks);
  };

  const pc = prog && prog.bytesTotal > 0 ? pct(prog.bytesDone, prog.bytesTotal) : 0;

  return (
    <section className="panel full dups">
      <div className="dup-bar">
        <label>
          Tamaño mínimo
          <select value={minIdx} disabled={running} onChange={(e) => setMinIdx(Number(e.target.value))}>
            {MIN_OPTIONS.map(([l], i) => (
              <option key={l} value={i}>
                {l}
              </option>
            ))}
          </select>
        </label>
        {running ? (
          <button className="danger" onClick={() => api.cancelDuplicates()}>
            Cancelar
          </button>
        ) : (
          <button className="primary" onClick={start}>
            {res ? "Buscar de nuevo" : "Buscar duplicados"}
          </button>
        )}
        <span className="muted dup-note">
          Compara el contenido de los archivos con el mismo tamaño. En unidades de red lee los archivos y puede tardar.
        </span>
      </div>

      {running && (
        <div className="dup-progress" role="status" aria-live="polite">
          <div>
            <b>{prog ? PHASE[prog.phase] : "Preparando…"}</b>
            {prog && prog.phase !== "tamaños" && prog.filesTotal > 0 && (
              <span>
                {" "}
                {fmtInt(prog.filesDone)} de {fmtInt(prog.filesTotal)} archivos · {fmtBytes(prog.bytesDone)} de {fmtBytes(prog.bytesTotal)}
              </span>
            )}
          </div>
          <div className="pbar big" aria-hidden="true">
            <i style={{ width: `${pc}%` }} />
          </div>
          {prog?.current && <div className="muted dup-cur">{prog.current}</div>}
        </div>
      )}

      {!running && !res && <div className="empty">Pulsa «Buscar duplicados» para empezar.</div>}

      {!running && res && (
        <>
          <div className="dup-summary">
            {res.cancelled ? (
              <b>Búsqueda cancelada.</b>
            ) : groups.length === 0 ? (
              <b>No se han encontrado duplicados de {MIN_OPTIONS.find((m) => m[1] === res.minSize)?.[0] ?? ""} o más.</b>
            ) : (
              <>
                <b>{fmtInt(groups.length)}</b> grupos de copias · se podrían liberar <b>{fmtBytes(wasted)}</b>
                {res.totalGroups > res.groups.length && <> (se muestran los {fmtInt(res.groups.length)} mayores de {fmtInt(res.totalGroups)})</>}
              </>
            )}
            {res.errors > 0 && <span className="muted"> · {fmtInt(res.errors)} archivos no se pudieron leer</span>}
          </div>

          {groups.length > 0 && (
            <>
              <div className="selbar dup-actions">
                <span>
                  <b>{fmtInt(sel.size)}</b> marcados · {fmtBytes(selSize)}
                </span>
                <button onClick={markAll}>Marcar todas las copias menos la más antigua</button>
                <button onClick={() => setSel(new Set())} disabled={sel.size === 0}>
                  Quitar marcas
                </button>
                <button className="danger solid" disabled={sel.size === 0 || allMarked > 0} onClick={doDelete}>
                  <Icon.trash /> Eliminar marcados…
                </button>
                {allMarked > 0 && <span className="warn-text">Deja al menos una copia sin marcar en cada grupo.</span>}
              </div>
              <div className="dup-list">
                {groups.map((g, gi) => {
                  const isOpen = open.has(gi);
                  const marked = g.files.filter((f) => sel.has(f.id)).length;
                  return (
                    <div key={g.files[0].id} className="dup-group">
                      <div className="dup-head">
                        <button
                          className="dup-toggle"
                          aria-expanded={isOpen}
                          onClick={() =>
                            setOpen((o) => {
                              const n = new Set(o);
                              if (n.has(gi)) n.delete(gi);
                              else n.add(gi);
                              return n;
                            })
                          }
                        >
                          <span className="arrow">{isOpen ? "▼" : "▶"}</span>
                          <b className="dup-name" title={g.files[0].name}>
                            {g.files[0].name}
                          </b>
                        </button>
                        <span className="muted">
                          {g.files.length} copias de {fmtBytes(g.size)}
                        </span>
                        <span className="dup-wasted">{fmtBytes(g.size * (g.files.length - 1))} recuperables</span>
                        <button onClick={() => markGroup(g, marked === 0)}>
                          {marked === 0 ? "Marcar copias" : "Quitar marcas"}
                        </button>
                      </div>
                      {isOpen && (
                        <ul className="dup-files">
                          {g.files.map((f, i) => (
                            <li key={f.id} className={sel.has(f.id) ? "sel" : ""}>
                              <label>
                                <input type="checkbox" checked={sel.has(f.id)} onChange={() => toggle(f.id)} />
                                <span className="dup-path" title={f.path}>
                                  {f.path}
                                </span>
                              </label>
                              <span className="muted dup-date">
                                {f.mtime > 0 ? fmtDate(f.mtime) : ""}
                                {i === 0 && " · la más antigua"}
                              </span>
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </>
      )}
    </section>
  );
}
