// Treemap «squarified» (Bruls, Huizing, van Wijk). Devuelve un rectángulo por elemento.

export interface Cell<T> {
  item: T;
  value: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Input<T> {
  item: T;
  value: number;
}

export function squarify<T>(input: Input<T>[], x: number, y: number, w: number, h: number): Cell<T>[] {
  const items = input.filter((i) => i.value > 0).sort((a, b) => b.value - a.value);
  const total = items.reduce((s, i) => s + i.value, 0);
  if (total <= 0 || w <= 0 || h <= 0) return [];

  const scale = (w * h) / total;
  const nodes = items.map((i) => ({ ...i, area: i.value * scale }));
  const out: Cell<T>[] = [];

  let rx = x;
  let ry = y;
  let rw = w;
  let rh = h;
  let row: typeof nodes = [];

  const worst = (r: typeof nodes, side: number) => {
    let sum = 0;
    let max = 0;
    let min = Infinity;
    for (const n of r) {
      sum += n.area;
      if (n.area > max) max = n.area;
      if (n.area < min) min = n.area;
    }
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
  };

  const flush = () => {
    if (row.length === 0) return;
    const sum = row.reduce((s, n) => s + n.area, 0);
    if (rw >= rh) {
      // columna a la izquierda
      const colW = sum / rh;
      let cy = ry;
      for (const n of row) {
        const hh = n.area / colW;
        out.push({ item: n.item, value: n.value, x: rx, y: cy, w: colW, h: hh });
        cy += hh;
      }
      rx += colW;
      rw -= colW;
    } else {
      // fila arriba
      const rowH = sum / rw;
      let cx = rx;
      for (const n of row) {
        const ww = n.area / rowH;
        out.push({ item: n.item, value: n.value, x: cx, y: ry, w: ww, h: rowH });
        cx += ww;
      }
      ry += rowH;
      rh -= rowH;
    }
    row = [];
  };

  for (const n of nodes) {
    const side = Math.min(rw, rh);
    if (row.length === 0 || worst([...row, n], side) <= worst(row, side)) {
      row.push(n);
    } else {
      flush();
      row = [n];
    }
  }
  flush();
  return out;
}
