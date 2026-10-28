// Uso: npm run version:set -- 1.2.3
// Pone la misma versión (MAYOR.MINOR.PATCH) en package.json, package-lock.json,
// src-tauri/Cargo.toml y src-tauri/Cargo.lock. Tauri toma la versión de Cargo.toml.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const v = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(v ?? "")) {
  console.error("Indica la versión como MAYOR.MINOR.PATCH, por ejemplo: npm run version:set -- 1.2.0");
  process.exit(1);
}

const edit = (file, fn) => {
  const p = join(root, file);
  const before = readFileSync(p, "utf8");
  const after = fn(before);
  if (after === before) console.log(`  = ${file} (ya estaba en ${v})`);
  else {
    writeFileSync(p, after);
    console.log(`  ✓ ${file}`);
  }
};

const json = (file) =>
  edit(file, (s) => {
    const o = JSON.parse(s);
    o.version = v;
    if (o.packages?.[""]) o.packages[""].version = v; // package-lock.json
    return JSON.stringify(o, null, 2) + "\n";
  });

console.log(`Versión → ${v}`);
json("package.json");
json("package-lock.json");
edit("src-tauri/Cargo.toml", (s) => s.replace(/^version = ".*"$/m, `version = "${v}"`));
edit("src-tauri/Cargo.lock", (s) =>
  s.replace(/(name = "sgiipp-diskanalizer"\nversion = ")[^"]*(")/, `$1${v}$2`),
);
console.log("Recuerda añadir la entrada en CHANGELOG.md y generar el zip como sgiipp-diskanalizer-" + v + ".zip");
