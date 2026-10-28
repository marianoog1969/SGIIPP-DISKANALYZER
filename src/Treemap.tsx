import { useEffect, useMemo, useRef, useState } from "react";
import type { NodeDto } from "./types";
import { squarify } from "./squarify";
import { CATEGORY_INFO, DIR_PALETTE, categoryOf, colorOf, type Category } from "./colors";
import { fmtBytes, fmtDate, fmtInt, joinPath, pct } from "./format";

interface Props {
  items: NodeDto[];
  restSize: number;
  restCount: number;
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
}

export default function Treemap({ items, restSize, restCount, basePath = "", live, highlightId, onHover, onOpen, onContext }: Props) {
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

  const total = useMemo(() => items.reduce((s, n) => s + n.size, 0) + restSize, [items, restSize]);

  const cells = useMemo(() => {
    if (dim.w < 10 || dim.h < 10) return [];
    const input = items.map((n) => ({ item: n as NodeDto | null, value: n.size }));
    if (restSize > 0) input.push({ item: null, value: restSize });
    return squarify(input, 0, 0, dim.w, dim.h);
  }, [items, restSize, dim]);

  const present = useMemo(() => {
    const s = new Set<Category>();
    items.forEach((n) => n.size > 0 && s.add(categoryOf(n.name, n.isDir)));
    return [...s];
  }, [items]);

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

  const show = (e: React.MouseEvent, n: NodeDto | null, value: number) =>
    setTip({ x: e.clientX, y: e.clientY, n, value });

  return (
    <div className="treemap-wrap">
      <div className={"treemap" + (live ? " live" : "")} ref={ref} onMouseLeave={() => {
          setTip(null);
          onHover?.(null);
        }}>
        {total === 0 && <div className="empty">{live ? "Leyendo…" : "Carpeta vacía"}</div>}
        {cells.map((c) => {
          const n = c.item;
          const label = n ? n.name : `${fmtInt(restCount)} elementos más`;
          const showName = c.w > 44 && c.h > 20;
          const showSize = c.w > 72 && c.h > 40;
          return (
            <div
              key={n ? n.id : "rest"}
              className={"cell" + (n?.isDir && !live ? " dir" : "") + (n ? "" : " rest") + (n && n.id === highlightId ? " hl" : "")}
              style={{
                left: c.x,
                top: c.y,
                width: c.w,
                height: c.h,
                background: n ? colorOf(n.name, n.isDir) : undefined,
              }}
              onMouseEnter={(e) => {
                show(e, n, c.value);
                onHover?.(n ? n.id : null);
              }}
              onMouseMove={(e) => show(e, n, c.value)}
              onClick={() => !live && n?.isDir && onOpen(n)}
              onContextMenu={(e) => {
                if (!n || !onContext) return;
                e.preventDefault();
                setTip(null);
                onContext(n, e.clientX, e.clientY);
              }}
            >
              {showName && <span className="cl-name">{label}</span>}
              {showSize && <span className="cl-size">{fmtBytes(c.value)}</span>}
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
          <b>{tip.n ? tip.n.name : `${fmtInt(restCount)} elementos más`}</b>
          {tip.n && !live && <span className="tip-path">{joinPath(basePath, tip.n.name)}</span>}
          <span className="tip-line">
            <b>{fmtBytes(tip.value)}</b> · {pct(tip.value, total).toFixed(1)} % de {fmtBytes(total)}
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
