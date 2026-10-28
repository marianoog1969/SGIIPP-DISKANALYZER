import { useEffect, useLayoutEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  hint?: string;
  danger?: boolean;
  onClick: () => void;
}

/** Entradas con `null` = separador. */
export type MenuEntries = (MenuItem | null)[];

interface Props {
  x: number;
  y: number;
  title: string;
  entries: MenuEntries;
  onClose: () => void;
}

export default function ContextMenu({ x, y, title, entries, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  // Que no se salga de la ventana.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      y: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    });
  }, [x, y, entries]);

  // `onClose` cambia en cada render: se guarda en una referencia para registrar los eventos una sola vez.
  // (Si se volvieran a registrar a mitad de una pulsación, Esc llegaría tarde y se perdería.)
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const close = () => closeRef.current();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation(); // Esc cierra solo el menú, no además la selección
        close();
      }
    };
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    window.addEventListener("wheel", close, { passive: true });
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("wheel", close);
      window.removeEventListener("keydown", key, true);
    };
  }, []);

  return (
    <div
      className="ctx-back"
      onMouseDown={onClose}
      onContextMenu={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div
        ref={ref}
        className="ctx"
        style={{ left: pos.x, top: pos.y }}
        role="menu"
        onMouseDown={(e) => e.stopPropagation()}
        onContextMenu={(e) => e.preventDefault()}
      >
        <div className="ctx-title" title={title}>
          {title}
        </div>
        {entries.map((it, i) =>
          it === null ? (
            <div key={i} className="ctx-sep" />
          ) : (
            <button
              key={i}
              role="menuitem"
              className={it.danger ? "danger" : ""}
              onClick={() => {
                onClose();
                it.onClick();
              }}
            >
              <span>{it.label}</span>
              {it.hint && <kbd>{it.hint}</kbd>}
            </button>
          ),
        )}
      </div>
    </div>
  );
}
