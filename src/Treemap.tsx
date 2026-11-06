import { useEffect, useMemo, useRef, useState } from "react";
import type { NodeDto, TmNode } from "./types";
import { squarify, type Cell } from "./squarify";
import { CATEGORY_INFO, DIR_PALETTE, categoryOf, colorOf, type Category } from "./colors";
import { fmtBytes, fmtDate, fmtInt, joinPath, pct } from "./format";

interface Props {
  /** Elementos del primer nivel (modo en vivo, o sin subcarpetas). */
  items: NodeDto[];
  restSize: number;
  restCount: number;
  /** Con esto el mapa muestra también el contenido de cada subcarpeta (ignora `items`). */
  tree?: TmNode | null;
  /** Ruta de la carpeta que se está viendo (para mostrar la ruta completa en el tooltip). */
  basePath?: string;
  /** Mientras se escanea: los datos cambian cada instante y no se puede entrar en las carpetas. */
  live?: boolean;
  /** Elemento resaltado desde fuera (p. ej. la fila sobre la que está el ratón en la tabla). */
  highlightId?: number | null;
  onHover?: (id: number | null) => void;
  onOpen: (n: NodeDto) => void;
  onContext?: (n: NodeDto, x: number, y: number) => void;
}

interface Tip {
  x: number;
  y: number;
  n: NodeDto | null;
  value: number;
  path: string;
  /** Carpeta que contiene al elemento, si no es del primer nivel. */
  inside?: { name: string; size: number };
}

interface Top {
  n: NodeDto;
  tm: TmNode | null;
}

/** Altura de la franja con el nombre de una carpeta que muestra su contenido. */
const HEAD = 18;
const MIN_NEST_W = 84;
const MIN_NEST_H = 52;

function dto(t: TmNode, parent: number): NodeDto {
  return { id: t.id, parent, name: t.name, isDir: t.isDir, size: t.size, files: t.files, mtime: t.mtime };
}

export default function Treemap({
  items,
  restSize,
  restCount,
  tree,
  basePath = "",
  live,
  highlightId,
  onHover,
  onOpen,
  onContext,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const [dim, setDim] = useState({ w: 0, h: 0 });
  const [tip, setTip] = useState<Tip | null>(null);
  const [tipPos, setTipPos] = useState({ x: 0, y: 0 });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) =>
      setDim({ w: Math.floor(e.contentRect.width), h: Math.floor(e.contentRect.height) }),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const useTree = !!tree && !live;
  const tops: Top[] = useMemo(
    () =>
      useTree
        ? tree!.children.map((c) => ({ n: dto(c, tree!.id), tm: c }))
        : items.map((n) => ({ n, tm: null })),
    [useTree, tree, items],
  );
  const rest = useTree ? { size: tree!.restSize, count: tree!.restCount } : { size: restSize, count: restCount };

  const total = useMemo(() => tops.reduce((s, t) => s + t.n.size, 0) + rest.size, [tops, rest.size]);

  const cells = useMemo(() => {
    if (dim.w < 10 || dim.h < 10) return [];
    const input = tops.map((t) => ({ item: t as Top | null, value: t.n.size }));
    if (rest.size > 0) input.push({ item: null, value: rest.size });
    return squarify(input, 0, 0, dim.w, dim.h);
  }, [tops, rest.size, dim]);

  // Dentro de cada carpeta suficientemente grande: sus propios hijos.
  const inner = useMemo(() => {
    const m = new Map<number, Cell<TmNode | null>[]>();
    if (!useTree) return m;
    for (const c of cells) {
      const t = c.item;
      if (!t?.tm || !t.tm.isDir || t.tm.children.length === 0) continue;
      if (c.w < MIN_NEST_W || c.h < MIN_NEST_H) continue;
      const iw = c.w - 4;
      const ih = c.h - HEAD - 3;
      const input = t.tm.children.map((k) => ({ item: k as TmNode | null, value: k.size }));
      if (t.tm.restSize > 0) input.push({ item: null, value: t.tm.restSize });
      m.set(t.n.id, squarify(input, 0, 0, iw, ih));
    }
    return m;
  }, [useTree, cells]);

  const present = useMemo(() => {
    const s = new Set<Category>();
    tops.forEach((t) => t.n.size > 0 && s.add(categoryOf(t.n.name, t.n.isDir)));
    inner.forEach((cs) =>
      cs.forEach((c) => c.item && c.item.size > 0 && s.add(categoryOf(c.item.name, c.item.isDir))),
    );
    return [...s];
  }, [tops, inner]);

  // El tooltip sigue al ratón sin salirse de la ventana.
  useEffect(() => {
    if (!tip) return;
    const w = tipRef.current?.offsetWidth ?? 280;
    const h = tipRef.current?.offsetHeight ?? 90;
    setTipPos({
      x: Math.max(6, Math.min(tip.x + 14, window.innerWidth - w - 6)),
      y: Math.max(6, Math.min(tip.y + 16, window.innerHeight - h - 6)),
    });
  }, [tip]);

  const show = (e: React.MouseEvent, n: NodeDto | null, value: number, path: string, inside?: { name: string; size: number }) =>
    setTip({ x: e.clientX, y: e.clientY, n, value, path, inside });

  return (
    <div className="treemap-wrap">
      <div
        className={"treemap" + (live ? " live" : "")}
        ref={ref}
        onMouseLeave={() => {
          setTip(null);
          onHover?.(null);
        }}
      >
        {total === 0 && <div className="empty">{live ? "Leyendo…" : "Carpeta vacía"}</div>}
        {cells.map((c) => {
          const t = c.item;
          const n = t ? t.n : null;
          const label = n ? n.name : `${fmtInt(rest.count)} elementos más`;
          const subs = n ? inner.get(n.id) : undefined;
          const showName = c.w > 44 && c.h > 20;
          const showSize = !subs && c.w > 72 && c.h > 40;
          const topPath = n ? joinPath(basePath, n.name) : "";
          return (
            <div
              key={n ? n.id : "rest"}
              className={
                "cell" +
                (n?.isDir && !live ? " dir" : "") +
                (n ? "" : " rest") +
                (subs ? " nest" : "") +
                (n && n.id === highlightId ? " hl" : "")
              }
              style={{
                left: c.x,
                top: c.y,
                width: c.w,
                height: c.h,
                background: n ? colorOf(n.name, n.isDir) : undefined,
              }}
              onMouseEnter={(e) => {
                show(e, n, c.value, topPath);
                onHover?.(n ? n.id : null);
              }}
              onMouseMove={(e) => show(e, n, c.value, topPath)}
              onClick={() => !live && n?.isDir && onOpen(n)}
              onContextMenu={(e) => {
                if (!n || !onContext) return;
                e.preventDefault();
                setTip(null);
                onContext(n, e.clientX, e.clientY);
              }}
            >
              {showName && (
                <span className="cl-name">
                  {label}
                  {subs && <em>{fmtBytes(c.value)}</em>}
                </span>
              )}
              {showSize && <span className="cl-size">{fmtBytes(c.value)}</span>}
              {subs && n && (
                <div className="nested" style={{ left: 2, top: HEAD, width: c.w - 4, height: c.h - HEAD - 3 }}>
                  {subs.map((s) => {
                    const k = s.item;
                    const sw = s.w > 40 && s.h > 15;
                    const sname = k ? k.name : `${fmtInt(t!.tm!.restCount)} más`;
                    return (
                      <div
                        key={k ? k.id : "r"}
                        className={"tm-sub" + (k ? "" : " rest") + (k?.isDir ? " dir" : "")}
                        style={{
                          left: s.x,
                          top: s.y,
                          width: s.w,
                          height: s.h,
                          background: k ? colorOf(k.name, k.isDir) : undefined,
                        }}
                        onMouseEnter={(e) => {
                          e.stopPropagation();
                          show(e, k ? dto(k, n.id) : null, s.value, k ? joinPath(topPath, k.name) : topPath, { name: n.name, size: n.size });
                          onHover?.(n.id);
                        }}
                        onMouseMove={(e) => {
                          e.stopPropagation();
                          show(e, k ? dto(k, n.id) : null, s.value, k ? joinPath(topPath, k.name) : topPath, { name: n.name, size: n.size });
                        }}
                        onClick={(e) => {
                          e.stopPropagation();
                          if (k?.isDir) onOpen(dto(k, n.id));
                        }}
                        onContextMenu={(e) => {
                          e.stopPropagation();
                          if (!k || !onContext) return;
                          e.preventDefault();
                          setTip(null);
                          onContext(dto(k, n.id), e.clientX, e.clientY);
                        }}
                      >
                        {sw && <span className="sub-name">{sname}</span>}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="legend">
        {present.map((c) => (
          <span key={c}>
            <i
              style={{
                background:
                  c === "dir"
                    ? `linear-gradient(90deg, ${DIR_PALETTE.slice(0, 3).join(", ")})`
                    : CATEGORY_INFO[c].color,
              }}
            />
            {c === "dir" ? (live ? "Carpetas" : "Carpetas (clic para entrar)") : CATEGORY_INFO[c].label}
          </span>
        ))}
      </div>
      {tip && (
        <div ref={tipRef} className="tip" style={{ left: tipPos.x, top: tipPos.y }}>
          <b>{tip.n ? tip.n.name : `${fmtInt(rest.count)} elementos más`}</b>
          {tip.n && !live && <span className="tip-path">{tip.path}</span>}
          <span className="tip-line">
            <b>{fmtBytes(tip.value)}</b> · {pct(tip.value, total).toFixed(1)} % de {fmtBytes(total)}
            {tip.inside && <> · {pct(tip.value, tip.inside.size).toFixed(0)} % de {tip.inside.name}</>}
          </span>
          {tip.n?.isDir && tip.n.files > 0 && <span className="tip-line">{fmtInt(tip.n.files)} archivos</span>}
          {tip.n && tip.n.mtime > 0 && <span className="tip-line">Modificado: {fmtDate(tip.n.mtime)}</span>}
          {tip.n && !live && (
            <span className="tip-hint">{tip.n.isDir ? "Clic: entrar · " : ""}clic derecho: más opciones</span>
          )}
        </div>
      )}
    </div>
  );
}
