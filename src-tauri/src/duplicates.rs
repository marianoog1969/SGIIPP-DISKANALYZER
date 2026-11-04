//! Búsqueda de archivos duplicados (mismo contenido), sin dependencias externas.
//!
//! Tres filtros sucesivos para leer lo mínimo posible (importante en red):
//! 1. mismo tamaño (ya se sabe por el escaneo, no cuesta leer nada);
//! 2. mismo principio de archivo (primeros 64 KiB);
//! 3. mismo contenido completo (huella de 128 bits).
//!
//! La huella no es criptográfica, pero con tamaño igual + 128 bits la probabilidad de
//! una coincidencia falsa es despreciable para este uso. Aun así, la interfaz avisa de
//! que el borrado es definitivo.

use crate::scan::Tree;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PARTIAL: u64 = 64 * 1024;
const BUF: usize = 1 << 20; // múltiplo de 8: las huellas no dependen de cómo parta el sistema las lecturas
const MAX_GROUPS: usize = 1000;
const TICK: Duration = Duration::from_millis(150);

pub struct DupGroup {
    pub size: u64,
    pub ids: Vec<u32>,
}

pub struct DupResult {
    pub groups: Vec<DupGroup>,
    /// Grupos encontrados en total (puede ser más que los devueltos).
    pub total_groups: usize,
    /// Espacio que se liberaría quedándose con una copia de cada grupo.
    pub wasted: u64,
    pub errors: u64,
    pub cancelled: bool,
    /// Archivos que se han tenido que comparar (los que comparten tamaño con otro).
    pub candidates: u64,
}

#[derive(Clone)]
pub struct DupProgress {
    /// "tamaños" | "inicio" | "contenido"
    pub phase: &'static str,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

// ───────────────────────────── Huella ─────────────────────────────

struct Hash2 {
    a: u64,
    b: u64,
    len: u64,
}

impl Hash2 {
    fn new() -> Self {
        Hash2 { a: 0x243F_6A88_85A3_08D3, b: 0x1319_8A2E_0370_7344, len: 0 }
    }

    #[inline]
    fn word(&mut self, w: u64) {
        self.a = (self.a ^ w).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(31);
        self.b = (self.b.wrapping_add(w)).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        self.b ^= self.b >> 29;
    }

    /// `data` debe venir en bloques de longitud múltiplo de 8 salvo el último.
    fn write(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        let mut chunks = data.chunks_exact(8);
        for c in &mut chunks {
            self.word(u64::from_le_bytes(c.try_into().expect("bloque de 8 bytes")));
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            let mut last = [0u8; 8];
            last[..rem.len()].copy_from_slice(rem);
            self.word(u64::from_le_bytes(last) ^ 0xA5A5_A5A5_A5A5_A5A5);
        }
    }

    fn finish(mut self) -> [u64; 2] {
        let l = self.len;
        self.word(l);
        // Mezcla final para que cada mitad dependa de la otra.
        let (a, b) = (self.a, self.b);
        [a ^ b.rotate_left(17), b ^ a.rotate_left(43)]
    }
}

/// Lee hasta llenar el búfer o hasta el final del archivo. Devuelve los bytes leídos.
fn fill(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Huella del archivo (o de sus primeros `limit` bytes). `Ok(None)` si se canceló.
fn hash_file(
    path: &PathBuf,
    limit: Option<u64>,
    cancel: &AtomicBool,
    on_bytes: &dyn Fn(u64),
) -> io::Result<Option<[u64; 2]>> {
    let mut f = File::open(path)?;
    let mut h = Hash2::new();
    let mut buf = vec![0u8; BUF];
    let mut left = limit.unwrap_or(u64::MAX);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let want = (buf.len() as u64).min(left) as usize;
        if want == 0 {
            break;
        }
        let n = fill(&mut f, &mut buf[..want])?;
        if n == 0 {
            break;
        }
        h.write(&buf[..n]);
        on_bytes(n as u64);
        left -= n as u64;
        if n < want {
            break;
        }
    }
    Ok(Some(h.finish()))
}

// ───────────────────────── Ejecución en paralelo ─────────────────────────

struct Reporter<'a, F: FnMut(&DupProgress)> {
    f: Mutex<(F, Instant)>,
    phase: &'static str,
    files_total: u64,
    bytes_total: u64,
    files_done: AtomicU64,
    bytes_done: AtomicU64,
    _l: std::marker::PhantomData<&'a ()>,
}

impl<F: FnMut(&DupProgress)> Reporter<'_, F> {
    fn emit(&self, current: &str, force: bool) {
        let mut g = self.f.lock().unwrap_or_else(|e| e.into_inner());
        if !force && g.1.elapsed() < TICK {
            return;
        }
        g.1 = Instant::now();
        let p = DupProgress {
            phase: self.phase,
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            bytes_done: self.bytes_done.load(Ordering::Relaxed),
            bytes_total: self.bytes_total,
            current: current.to_string(),
        };
        (g.0)(&p);
    }
}

type Item = (u32, PathBuf, u64);

/// Calcula la huella de cada elemento en `threads` hilos. Devuelve (id → huella) y el número de errores.
fn hash_all<F: FnMut(&DupProgress) + Send>(
    items: &[Item],
    limit: Option<u64>,
    phase: &'static str,
    threads: usize,
    cancel: &AtomicBool,
    progress: &Mutex<F>,
) -> (HashMap<u32, [u64; 2]>, u64) {
    let bytes_total: u64 = items.iter().map(|i| limit.map_or(i.2, |l| i.2.min(l))).sum();
    // El callback del llamador se comparte entre fases a través de un `Mutex` externo.
    let rep = Reporter {
        f: Mutex::new((
            |p: &DupProgress| {
                let mut g = progress.lock().unwrap_or_else(|e| e.into_inner());
                (g)(p)
            },
            Instant::now() - TICK,
        )),
        phase,
        files_total: items.len() as u64,
        bytes_total,
        files_done: AtomicU64::new(0),
        bytes_done: AtomicU64::new(0),
        _l: std::marker::PhantomData,
    };
    let next = AtomicUsize::new(0);
    let out: Mutex<HashMap<u32, [u64; 2]>> = Mutex::new(HashMap::new());
    let errors = AtomicU64::new(0);

    std::thread::scope(|s| {
        for _ in 0..threads.clamp(1, 64) {
            s.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((id, path, _)) = items.get(i) else { return };
                let name = path.to_string_lossy();
                let tick = |n: u64| {
                    rep.bytes_done.fetch_add(n, Ordering::Relaxed);
                    rep.emit(&name, false);
                };
                match hash_file(path, limit, cancel, &tick) {
                    Ok(Some(h)) => {
                        out.lock().unwrap_or_else(|e| e.into_inner()).insert(*id, h);
                    }
                    Ok(None) => return,
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                rep.files_done.fetch_add(1, Ordering::Relaxed);
                rep.emit(&name, false);
            });
        }
    });
    rep.emit("", true);
    (out.into_inner().unwrap_or_else(|e| e.into_inner()), errors.load(Ordering::Relaxed))
}

/// Agrupa por una clave y se queda con los grupos de 2 o más elementos.
fn keep_groups<K: std::hash::Hash + Eq>(items: Vec<Item>, key: impl Fn(&Item) -> Option<K>) -> Vec<Vec<Item>> {
    let mut map: HashMap<K, Vec<Item>> = HashMap::new();
    for it in items {
        if let Some(k) = key(&it) {
            map.entry(k).or_default().push(it);
        }
    }
    map.into_values().filter(|g| g.len() > 1).collect()
}

/// Busca duplicados entre los archivos del árbol que midan al menos `min_size` bytes.
pub fn find<F: FnMut(&DupProgress) + Send>(
    tree: &Tree,
    min_size: u64,
    threads: usize,
    cancel: &AtomicBool,
    progress: F,
) -> DupResult {
    let progress = Mutex::new(progress);
    let mut errors = 0u64;

    // 1) mismo tamaño
    let mut by_size: HashMap<u64, Vec<u32>> = HashMap::new();
    for (i, n) in tree.nodes.iter().enumerate() {
        if !n.is_dir && !n.removed && n.size > 0 && n.size >= min_size {
            by_size.entry(n.size).or_default().push(i as u32);
        }
    }
    let mut cands: Vec<Item> = Vec::new();
    for (size, ids) in by_size {
        if ids.len() > 1 {
            for id in ids {
                cands.push((id, tree.full_path(id), size));
            }
        }
    }
    let candidates = cands.len() as u64;
    {
        let mut g = progress.lock().unwrap_or_else(|e| e.into_inner());
        (g)(&DupProgress {
            phase: "tamaños",
            files_done: candidates,
            files_total: candidates,
            bytes_done: 0,
            bytes_total: 0,
            current: String::new(),
        });
    }

    // 2) mismo principio de archivo
    let (partial, e1) = hash_all(&cands, Some(PARTIAL), "inicio", threads, cancel, &progress);
    errors += e1;
    if cancel.load(Ordering::Relaxed) {
        return DupResult { groups: vec![], total_groups: 0, wasted: 0, errors, cancelled: true, candidates };
    }
    let stage2 = keep_groups(cands, |it| partial.get(&it.0).map(|h| (it.2, *h)));

    // 3) contenido completo (los archivos de hasta 64 KiB ya están comparados por completo)
    let mut confirmed: Vec<Vec<Item>> = Vec::new();
    let (small, big): (Vec<Vec<Item>>, Vec<Vec<Item>>) = stage2.into_iter().partition(|g| g[0].2 <= PARTIAL);
    confirmed.extend(small);
    let to_hash: Vec<Item> = big.into_iter().flatten().collect();
    let (full, e2) = hash_all(&to_hash, None, "contenido", threads, cancel, &progress);
    errors += e2;
    if cancel.load(Ordering::Relaxed) {
        return DupResult { groups: vec![], total_groups: 0, wasted: 0, errors, cancelled: true, candidates };
    }
    confirmed.extend(keep_groups(to_hash, |it| full.get(&it.0).map(|h| (it.2, *h))));

    // Resultado: más espacio recuperable primero.
    let mut groups: Vec<DupGroup> = confirmed
        .into_iter()
        .map(|g| {
            let size = g[0].2;
            let mut ids: Vec<u32> = g.into_iter().map(|i| i.0).collect();
            // La copia «original» probable (la más antigua) queda primero.
            ids.sort_by(|&a, &b| {
                let (x, y) = (&tree.nodes[a as usize], &tree.nodes[b as usize]);
                x.mtime.cmp(&y.mtime).then(a.cmp(&b))
            });
            DupGroup { size, ids }
        })
        .collect();
    groups.sort_by(|a, b| {
        let wa = a.size * (a.ids.len() as u64 - 1);
        let wb = b.size * (b.ids.len() as u64 - 1);
        wb.cmp(&wa).then(a.ids[0].cmp(&b.ids[0]))
    });
    let wasted = groups.iter().map(|g| g.size * (g.ids.len() as u64 - 1)).sum();
    let total_groups = groups.len();
    groups.truncate(MAX_GROUPS);
    DupResult { groups, total_groups, wasted, errors, cancelled: false, candidates }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;
    use std::fs;
    use std::sync::Arc;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("sgiipp_dup_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn scan_dir(p: &PathBuf) -> Tree {
        scan::scan(p.to_str().unwrap(), 4, Arc::new(AtomicBool::new(false)), |_| {}).unwrap()
    }

    #[test]
    fn finds_exact_duplicates_only() {
        let d = tmp("a");
        fs::create_dir_all(d.join("sub")).unwrap();
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut almost = big.clone();
        let last = almost.len() - 1;
        almost[last] ^= 1; // mismo tamaño y mismo principio, distinto final
        fs::write(d.join("a.bin"), &big).unwrap();
        fs::write(d.join("sub/copia_de_a.bin"), &big).unwrap();
        fs::write(d.join("casi.bin"), &almost).unwrap();
        fs::write(d.join("pequeno1.txt"), b"hola mundo").unwrap();
        fs::write(d.join("sub/pequeno2.txt"), b"hola mundo").unwrap();
        fs::write(d.join("otro.txt"), b"hola mundO").unwrap(); // mismo tamaño, distinto contenido
        fs::write(d.join("vacio1"), b"").unwrap();
        fs::write(d.join("vacio2"), b"").unwrap();
        let t = scan_dir(&d);
        let r = find(&t, 0, 4, &AtomicBool::new(false), |_| {});
        assert!(!r.cancelled);
        assert_eq!(r.errors, 0);
        assert_eq!(r.total_groups, 2, "un grupo grande y uno pequeño; los vacíos no cuentan");
        let names: Vec<Vec<String>> = r
            .groups
            .iter()
            .map(|g| {
                let mut v: Vec<String> = g.ids.iter().map(|&i| t.nodes[i as usize].name.to_string()).collect();
                v.sort();
                v
            })
            .collect();
        assert_eq!(names[0], vec!["a.bin", "copia_de_a.bin"], "{names:?}");
        assert_eq!(names[1], vec!["pequeno1.txt", "pequeno2.txt"]);
        assert_eq!(r.wasted, 300_000 + 10);
        // Con tamaño mínimo se descartan los pequeños.
        let r2 = find(&t, 1000, 2, &AtomicBool::new(false), |_| {});
        assert_eq!(r2.total_groups, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn can_be_cancelled_and_reports_progress() {
        let d = tmp("b");
        let data = vec![7u8; 200_000];
        for i in 0..6 {
            fs::write(d.join(format!("f{i}.dat")), &data).unwrap();
        }
        let t = scan_dir(&d);
        let mut phases = std::collections::BTreeSet::new();
        let r = find(&t, 0, 3, &AtomicBool::new(false), |p| {
            phases.insert(p.phase);
        });
        assert_eq!(r.total_groups, 1);
        assert_eq!(r.groups[0].ids.len(), 6);
        assert!(phases.contains("contenido") && phases.contains("inicio"), "{phases:?}");
        let r = find(&t, 0, 3, &AtomicBool::new(true), |_| {});
        assert!(r.cancelled && r.groups.is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hash_does_not_depend_on_read_sizes() {
        let d = tmp("c");
        let data: Vec<u8> = (0..(BUF + 12_345) as u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
        fs::write(d.join("x"), &data).unwrap();
        let p = d.join("x");
        let cancel = AtomicBool::new(false);
        let h1 = hash_file(&p, None, &cancel, &|_| {}).unwrap().unwrap();
        let h2 = hash_file(&p, None, &cancel, &|_| {}).unwrap().unwrap();
        assert_eq!(h1, h2);
        let mut other = data.clone();
        other[BUF + 100] ^= 0x10;
        fs::write(d.join("y"), &other).unwrap();
        let h3 = hash_file(&d.join("y"), None, &cancel, &|_| {}).unwrap().unwrap();
        assert_ne!(h1, h3);
        let _ = fs::remove_dir_all(&d);
    }
}
