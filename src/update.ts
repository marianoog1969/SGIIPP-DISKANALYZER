/** Aviso de versiones nuevas: consulta la última «Release» publicada en GitHub. */

export const REPO = "marianoog1969/SGIIPP-DISKANALYZER";
const API = `https://api.github.com/repos/${REPO}/releases/latest`;

export interface UpdateInfo {
  /** Versión publicada (sin la «v»). */
  latest: string;
  /** Es posterior a la instalada. */
  newer: boolean;
}

/** Convierte «v1.6.3» o «1.6.3» en [1, 6, 3]; null si no tiene ese formato. */
export function parseVersion(v: string): number[] | null {
  const m = /^v?(\d+)\.(\d+)\.(\d+)/.exec(v.trim());
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** ¿`a` es una versión posterior a `b`? */
export function isNewer(a: string, b: string): boolean {
  const x = parseVersion(a);
  const y = parseVersion(b);
  if (!x || !y) return false;
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
}

/**
 * Pregunta a GitHub cuál es la última versión. Devuelve null si todavía no hay ninguna
 * publicada; lanza un error si no se puede consultar (sin red, bloqueado, etc.).
 */
export async function checkUpdate(current: string, timeoutMs = 8000): Promise<UpdateInfo | null> {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), timeoutMs);
  try {
    const r = await fetch(API, { headers: { Accept: "application/vnd.github+json" }, signal: ctl.signal, cache: "no-store" });
    if (r.status === 404) return null;
    if (!r.ok) throw new Error(`GitHub respondió ${r.status}`);
    const j = (await r.json()) as { tag_name?: string };
    const tag = String(j.tag_name ?? "");
    if (!parseVersion(tag)) return null;
    const latest = tag.replace(/^v/i, "");
    return { latest, newer: isNewer(latest, current) };
  } finally {
    clearTimeout(timer);
  }
}
