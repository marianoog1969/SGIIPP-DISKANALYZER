export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB", "PB"];
  let v = n;
  let i = -1;
  do {
    v /= 1024;
    i++;
  } while (v >= 1024 && i < units.length - 1);
  const digits = v >= 100 ? 0 : v >= 10 ? 1 : 2;
  return `${v.toLocaleString("es-ES", { maximumFractionDigits: digits, minimumFractionDigits: digits })} ${units[i]}`;
}

export const fmtInt = (n: number) => n.toLocaleString("es-ES");

export function fmtDate(sec: number): string {
  if (!sec || sec <= 0) return "—";
  return new Date(sec * 1000)
    .toLocaleString("es-ES", {
      day: "2-digit",
      month: "2-digit",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    })
    .replace(",", "");
}

export function fmtDur(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${s % 60} s`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

export function shortenPath(p: string, max = 96): string {
  if (p.length <= max) return p;
  return `${p.slice(0, 30)} … ${p.slice(-(max - 33))}`;
}

export function pct(part: number, total: number): number {
  return total > 0 ? (part / total) * 100 : 0;
}

export function fmtPct(p: number): string {
  return `${p.toLocaleString("es-ES", { minimumFractionDigits: 1, maximumFractionDigits: 1 })} %`;
}

/** Une una carpeta y un nombre con la barra de Windows (sin duplicarla en «C:\\»). */
export function joinPath(base: string, name: string): string {
  if (!base) return name;
  return /[\\/]$/.test(base) ? base + name : `${base}\\${name}`;
}
