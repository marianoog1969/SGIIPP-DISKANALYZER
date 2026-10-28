//! Escáner paralelo de carpetas.
//!
//! No depende de Tauri: se puede probar aislado con `cargo test`.
//!
//! Diseño: un pool de hilos consume una pila de carpetas pendientes. Cada hilo
//! lee una carpeta completa (una sola enumeración; en Windows el tamaño y la
//! fecha vienen en la propia enumeración, sin ir de nuevo a la red por archivo),
//! inserta los hijos en un arena compartido y encola las subcarpetas.
//! Al terminar, un recorrido inverso del arena suma los tamaños hacia arriba.

use std::cmp::{Ordering as Cmp, Reverse};
use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

pub const NO_PARENT: u32 = u32::MAX;
const MAX_ERRORS_KEPT: usize = 1000;
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

// ───────────────────────────── Datos ─────────────────────────────

pub struct Node {
    pub name: Box<str>,
    pub parent: u32,
    pub is_dir: bool,
    /// Tamaño lógico en bytes (en carpetas: suma de todo lo que contienen).
    pub size: u64,
    /// Solo carpetas: número de archivos que contienen (recursivo).
    pub files: u64,
    /// Última modificación (segundos Unix). En carpetas: la más reciente de su contenido.
    pub mtime: i64,
    pub children: Vec<u32>,
    /// Eliminado después del escaneo (se conserva en el arena para no renumerar).
    pub removed: bool,
}

#[allow(dead_code)] // `files` y `dirs` los usan las pruebas
pub struct Removed {
    pub size: u64,
    pub files: u64,
    pub dirs: u64,
}

pub struct ScanError {
    pub path: String,
    pub message: String,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub errors: Vec<ScanError>,
    pub error_count: u64,
    /// Subcarpetas encontradas (sin contar la raíz).
    pub dirs: u64,
    pub files: u64,
    pub elapsed: Duration,
    pub cancelled: bool,
}

/// Entrada de primer nivel (hija directa de la raíz) con lo contado hasta ahora.
#[derive(Clone, Debug)]
pub struct LiveItem {
    pub name: Box<str>,
    pub is_dir: bool,
    pub size: u64,
    pub files: u64,
}

/// Cuántas entradas de primer nivel se envían en cada aviso de progreso.
const LIVE_TOP: usize = 150;

#[derive(Clone, Debug)]
pub struct Progress {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
    pub current: String,
    pub elapsed: Duration,
    /// Las entradas de primer nivel más grandes hasta ahora (de mayor a menor).
    pub top: Vec<LiveItem>,
    /// Cuántas entradas de primer nivel hay en total y cuánto suman las que no van en `top`.
    pub top_total: usize,
    pub top_rest_size: u64,
}

/// Traduce un error de E/S a un motivo que entienda cualquiera.
pub fn friendly_io(e: &io::Error) -> String {
    let code = e.raw_os_error();
    let text = match code {
        Some(5) => Some("Sin permisos (acceso denegado)"),
        Some(32) | Some(33) => Some("En uso por otro programa o usuario"),
        Some(2) | Some(3) => Some("Ya no existe"),
        Some(53) | Some(55) | Some(64) | Some(67) | Some(1219) | Some(1231) => {
            Some("Se perdió la conexión con el recurso de red")
        }
        Some(121) => Some("La red no responde (tiempo agotado)"),
        Some(145) => Some("La carpeta no se pudo vaciar del todo (algo la está usando)"),
        Some(206) => Some("Ruta demasiado larga"),
        Some(1920) => Some("Archivo o enlace inaccesible"),
        _ => None,
    };
    let text = text.or(match e.kind() {
        io::ErrorKind::PermissionDenied => Some("Sin permisos (acceso denegado)"),
        io::ErrorKind::NotFound => Some("Ya no existe"),
        _ => None,
    });
    match (text, code) {
        (Some(t), Some(c)) => format!("{t} (error {c})"),
        (Some(t), None) => t.to_string(),
        (None, _) => e.to_string(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Size,
    Name,
    Files,
    Mtime,
}

impl SortKey {
    pub fn parse(s: &str) -> Self {
        match s {
            "name" => SortKey::Name,
            "files" => SortKey::Files,
            "mtime" => SortKey::Mtime,
            _ => SortKey::Size,
        }
    }
}

pub struct Page {
    pub ids: Vec<u32>,
    pub total: usize,
    pub rest_count: usize,
    pub rest_size: u64,
}

pub struct ExtStat {
    /// Vacío = sin extensión.
    pub ext: String,
    pub count: u64,
    pub size: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Files,
    Dirs,
    All,
}

impl Kind {
    pub fn parse(s: &str) -> Self {
        match s {
            "dirs" => Kind::Dirs,
            "all" => Kind::All,
            _ => Kind::Files,
        }
    }
}

/// Filtros de la búsqueda. Todos son opcionales y se combinan con «y».
pub struct Query {
    /// Texto del nombre. Sin `*` ni `?` se busca como fragmento; con ellos, como patrón.
    pub text: String,
    pub min_size: u64,
    /// Solo lo modificado antes de este instante (segundos Unix).
    pub modified_before: Option<i64>,
    /// Extensiones sin punto, en minúsculas. Vacío = todas. (Solo afectan a archivos.)
    pub exts: Vec<String>,
    pub kind: Kind,
}

pub struct SearchResult {
    pub ids: Vec<u32>,
    pub count: u64,
    /// Suma del tamaño de lo encontrado. Solo si se buscan archivos (con carpetas se contaría dos veces).
    pub total_size: Option<u64>,
}

/// Coincidencia con comodines `*` (cualquier cosa) y `?` (un carácter), sin distinguir mayúsculas.
/// `pat` y `text` ya vienen en minúsculas.
fn glob(pat: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while t < text.len() {
        if p < pat.len() && (pat[p] == '?' || pat[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pat.len() && pat[p] == '*' {
            star = p;
            mark = t;
            p += 1;
        } else if star != usize::MAX {
            p = star + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == '*' {
        p += 1;
    }
    p == pat.len()
}

fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(p) if p > 0 && p + 1 < name.len() && name.len() - p - 1 <= 16 => &name[p + 1..],
        _ => "",
    }
}

fn name_cmp(a: &Node, b: &Node) -> Cmp {
    a.name
        .chars()
        .flat_map(char::to_lowercase)
        .cmp(b.name.chars().flat_map(char::to_lowercase))
}

impl Tree {
    /// Ruta completa de un nodo (la raíz conserva el texto con el que se escaneó).
    pub fn full_path(&self, id: u32) -> PathBuf {
        let mut chain = vec![id];
        let mut cur = id;
        while self.nodes[cur as usize].parent != NO_PARENT {
            cur = self.nodes[cur as usize].parent;
            chain.push(cur);
        }
        let mut it = chain.into_iter().rev();
        let root = it.next().expect("la cadena nunca está vacía");
        let mut path = PathBuf::from(&*self.nodes[root as usize].name);
        for i in it {
            path.push(&*self.nodes[i as usize].name);
        }
        path
    }

    /// Cadena de ancestros desde la raíz hasta el propio nodo (incluido).
    pub fn ancestors(&self, id: u32) -> Vec<u32> {
        let mut chain = vec![id];
        let mut cur = id;
        while self.nodes[cur as usize].parent != NO_PARENT {
            cur = self.nodes[cur as usize].parent;
            chain.push(cur);
        }
        chain.reverse();
        chain
    }

    /// Hijos de una carpeta, ordenados, con tope de `limit` elementos.
    /// Lo que queda fuera se resume en `rest_count` / `rest_size`.
    pub fn children_page(&self, id: u32, key: SortKey, desc: bool, limit: usize) -> Option<Page> {
        let node = self.nodes.get(id as usize).filter(|n| !n.removed)?;
        let mut ids = node.children.clone();
        let total = ids.len();
        let nodes = &self.nodes;
        let cmp = |a: &u32, b: &u32| -> Cmp {
            let (x, y) = (&nodes[*a as usize], &nodes[*b as usize]);
            let o = match key {
                SortKey::Size => x.size.cmp(&y.size),
                SortKey::Files => x.files.cmp(&y.files),
                SortKey::Mtime => x.mtime.cmp(&y.mtime),
                SortKey::Name => name_cmp(x, y),
            };
            let o = if desc { o.reverse() } else { o };
            o.then_with(|| name_cmp(x, y)).then_with(|| a.cmp(b))
        };
        let take = limit.min(total);
        let mut rest_size = 0u64;
        let mut rest_count = 0usize;
        if take < total {
            ids.select_nth_unstable_by(take, &cmp);
            let rest = ids.split_off(take);
            rest_count = rest.len();
            rest_size = rest.iter().map(|&i| nodes[i as usize].size).sum();
        }
        ids.sort_unstable_by(&cmp);
        Some(Page { ids, total, rest_count, rest_size })
    }

    /// Quita un elemento (y todo lo que contiene) del árbol en memoria y resta su
    /// tamaño de todos sus ancestros. No toca el disco. La raíz no se puede quitar.
    pub fn remove(&mut self, id: u32) -> Option<Removed> {
        let idx = id as usize;
        if id == 0 || idx >= self.nodes.len() || self.nodes[idx].removed {
            return None;
        }
        let (size, is_dir, parent) = {
            let n = &self.nodes[idx];
            (n.size, n.is_dir, n.parent)
        };
        let files = if is_dir { self.nodes[idx].files } else { 1 };

        let mut dirs = 0u64;
        let mut stack = vec![id];
        while let Some(i) = stack.pop() {
            let n = &mut self.nodes[i as usize];
            n.removed = true;
            if n.is_dir {
                dirs += 1;
                stack.extend(std::mem::take(&mut n.children));
            }
        }

        let mut p = parent;
        while p != NO_PARENT {
            let a = &mut self.nodes[p as usize];
            a.size = a.size.saturating_sub(size);
            a.files = a.files.saturating_sub(files);
            p = a.parent;
        }
        self.nodes[parent as usize].children.retain(|&c| c != id);
        self.files = self.files.saturating_sub(files);
        self.dirs = self.dirs.saturating_sub(dirs);
        Some(Removed { size, files, dirs })
    }

    /// Sustituye la carpeta `old_id` por `sub`, un escaneo reciente de esa misma carpeta
    /// (así el árbol refleja lo que hay ahora en disco). El id de la carpeta se
    /// conserva; los de su contenido cambian. Ajusta los totales de todos los ancestros.
    pub fn graft(&mut self, old_id: u32, sub: Tree) -> bool {
        let idx = old_id as usize;
        if idx >= self.nodes.len() || self.nodes[idx].removed || !self.nodes[idx].is_dir {
            return false;
        }
        let (parent, old_size, old_files, name) = {
            let n = &self.nodes[idx];
            (n.parent, n.size, n.files, n.name.clone())
        };

        // Contenido antiguo fuera.
        let mut old_dirs = 1u64; // la propia carpeta
        let mut stack = std::mem::take(&mut self.nodes[idx].children);
        while let Some(i) = stack.pop() {
            let n = &mut self.nodes[i as usize];
            n.removed = true;
            if n.is_dir {
                old_dirs += 1;
                stack.extend(std::mem::take(&mut n.children));
            }
        }

        // Contenido nuevo dentro: la raíz del escaneo ocupa el hueco de la carpeta.
        let (new_size, new_files, mut new_dirs) = (sub.nodes[0].size, sub.nodes[0].files, sub.dirs + 1);
        // `self.dirs` no cuenta la raíz del árbol: si se injerta la propia raíz, no se suma ni se resta.
        if parent == NO_PARENT {
            old_dirs -= 1;
            new_dirs -= 1;
        }
        let off = self.nodes.len() as u32;
        let map = |i: u32| if i == 0 { old_id } else { off + i - 1 };
        let mut it = sub.nodes.into_iter();
        let mut root = it.next().expect("un escaneo siempre tiene raíz");
        root.name = name;
        root.parent = parent;
        root.children = root.children.iter().map(|&c| map(c)).collect();
        self.nodes[idx] = root;
        for mut n in it {
            n.parent = map(n.parent);
            for c in n.children.iter_mut() {
                *c = map(*c);
            }
            self.nodes.push(n);
        }

        let mut p = parent;
        while p != NO_PARENT {
            let a = &mut self.nodes[p as usize];
            a.size = a.size.saturating_sub(old_size) + new_size;
            a.files = a.files.saturating_sub(old_files) + new_files;
            p = a.parent;
        }
        self.files = self.files.saturating_sub(old_files) + new_files;
        self.dirs = self.dirs.saturating_sub(old_dirs) + new_dirs;
        self.error_count += sub.error_count;
        for e in sub.errors {
            if self.errors.len() < MAX_ERRORS_KEPT {
                self.errors.push(e);
            }
        }
        true
    }

    /// Los `k` archivos más grandes de todo el árbol.
    pub fn top_files(&self, k: usize) -> Vec<u32> {
        if k == 0 {
            return Vec::new();
        }
        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(k + 1);
        for (i, n) in self.nodes.iter().enumerate() {
            if n.is_dir || n.removed {
                continue;
            }
            if heap.len() < k {
                heap.push(Reverse((n.size, i as u32)));
            } else if let Some(&Reverse((min, _))) = heap.peek() {
                if n.size > min {
                    heap.pop();
                    heap.push(Reverse((n.size, i as u32)));
                }
            }
        }
        let mut v: Vec<(u64, u32)> = heap.into_iter().map(|Reverse(x)| x).collect();
        v.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        v.into_iter().map(|(_, i)| i).collect()
    }

    /// Busca por nombre, tamaño, antigüedad y extensión. Devuelve los `limit` más grandes.
    pub fn search(&self, q: &Query, limit: usize) -> SearchResult {
        let lower = |s: &str| -> Vec<char> { s.chars().flat_map(char::to_lowercase).collect() };
        let text = q.text.trim();
        let wild = text.contains('*') || text.contains('?');
        let needle = lower(text);
        let needle_str: String = needle.iter().collect();

        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(limit + 1);
        let (mut count, mut total) = (0u64, 0u64);
        let mut buf = String::new();

        for (i, n) in self.nodes.iter().enumerate().skip(1) {
            if n.removed {
                continue;
            }
            match q.kind {
                Kind::Files if n.is_dir => continue,
                Kind::Dirs if !n.is_dir => continue,
                _ => {}
            }
            if n.size < q.min_size {
                continue;
            }
            if let Some(cut) = q.modified_before {
                // Una carpeta vacía no tiene fecha útil de contenido; mtime 0 = desconocida.
                if n.mtime == 0 || n.mtime >= cut {
                    continue;
                }
            }
            if !q.exts.is_empty() {
                if n.is_dir {
                    continue;
                }
                buf.clear();
                buf.extend(ext_of(&n.name).chars().flat_map(char::to_lowercase));
                if !q.exts.iter().any(|e| *e == buf) {
                    continue;
                }
            }
            if !needle.is_empty() {
                buf.clear();
                buf.extend(n.name.chars().flat_map(char::to_lowercase));
                let ok = if wild {
                    let t: Vec<char> = buf.chars().collect();
                    glob(&needle, &t)
                } else {
                    buf.contains(needle_str.as_str())
                };
                if !ok {
                    continue;
                }
            }
            count += 1;
            if !n.is_dir {
                total += n.size;
            }
            if heap.len() < limit {
                heap.push(Reverse((n.size, i as u32)));
            } else if let Some(&Reverse((min, _))) = heap.peek() {
                if n.size > min {
                    heap.pop();
                    heap.push(Reverse((n.size, i as u32)));
                }
            }
        }
        let mut v: Vec<(u64, u32)> = heap.into_iter().map(|Reverse(x)| x).collect();
        v.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        SearchResult {
            ids: v.into_iter().map(|(_, i)| i).collect(),
            count,
            total_size: if q.kind == Kind::Files { Some(total) } else { None },
        }
    }

    /// Espacio por extensión, de mayor a menor.
    pub fn extensions(&self, limit: usize) -> Vec<ExtStat> {
        let mut map: HashMap<String, (u64, u64)> = HashMap::new();
        let mut buf = String::new();
        for n in &self.nodes {
            if n.is_dir || n.removed {
                continue;
            }
            buf.clear();
            if let Some(p) = n.name.rfind('.') {
                let ext = &n.name[p + 1..];
                if p > 0 && !ext.is_empty() && ext.len() <= 16 {
                    buf.extend(ext.chars().flat_map(char::to_lowercase));
                }
            }
            match map.get_mut(buf.as_str()) {
                Some(e) => {
                    e.0 += 1;
                    e.1 += n.size;
                }
                None => {
                    map.insert(buf.clone(), (1, n.size));
                }
            }
        }
        let mut v: Vec<ExtStat> = map
            .into_iter()
            .map(|(ext, (count, size))| ExtStat { ext, count, size })
            .collect();
        v.sort_unstable_by(|a, b| b.size.cmp(&a.size).then_with(|| a.ext.cmp(&b.ext)));
        v.truncate(limit);
        v
    }
}

// ─────────────────────────── Entrada ───────────────────────────

/// Limpia lo que escribe el usuario y valida lo evidente.
pub fn normalize_root(input: &str) -> Result<PathBuf, String> {
    let mut s = input.trim().trim_matches('"').trim().to_string();
    if s.is_empty() {
        return Err("Indica una carpeta o unidad para escanear.".into());
    }
    if s.len() > 3 {
        s = s.trim_end_matches(&['\\', '/'][..]).to_string();
    }
    let b = s.as_bytes();
    if b.len() == 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        s.push('\\'); // «C:» significaría «carpeta actual de C», no la raíz
    }
    if s.starts_with("\\\\") && !s.starts_with("\\\\?\\") {
        let parts = s[2..].split('\\').filter(|p| !p.is_empty()).count();
        if parts < 2 {
            return Err(
                "En una ruta de red indica servidor y recurso compartido, por ejemplo \\\\servidor\\recurso"
                    .into(),
            );
        }
    }
    Ok(PathBuf::from(s))
}

fn mtime_of(m: &fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ─────────────────────────── Escaneo ───────────────────────────

const NO_LIVE: usize = usize::MAX;

struct Task {
    id: u32,
    path: PathBuf,
    /// Posición de su entrada de primer nivel en `Shared::live` (`NO_LIVE` para la raíz).
    live: usize,
}

struct Queue {
    items: Vec<Task>, // pila: se explora en profundidad y la cola se mantiene pequeña
    pending: usize,   // carpetas en cola + en proceso
}

struct Shared {
    arena: Mutex<Vec<Node>>,
    queue: Mutex<Queue>,
    work_cv: Condvar,
    done_cv: Condvar,
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    error_count: AtomicU64,
    errors: Mutex<Vec<ScanError>>,
    current: Mutex<String>,
    /// Primer nivel del árbol con tamaños que van creciendo mientras se escanea.
    live: Mutex<Vec<LiveItem>>,
    cancel: Arc<AtomicBool>,
}

struct TaskGuard<'a>(&'a Shared);

impl Drop for TaskGuard<'_> {
    fn drop(&mut self) {
        let mut q = self.0.queue.lock().unwrap_or_else(|e| e.into_inner());
        q.pending -= 1;
        if q.pending == 0 {
            self.0.work_cv.notify_all();
            self.0.done_cv.notify_all();
        }
    }
}

impl Shared {
    fn record_error(&self, path: &Path, err: &io::Error) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
        let mut v = self.errors.lock().unwrap_or_else(|e| e.into_inner());
        if v.len() < MAX_ERRORS_KEPT {
            v.push(ScanError {
                path: path.to_string_lossy().into_owned(),
                message: friendly_io(err),
            });
        }
    }

    fn snapshot(&self, started: Instant) -> Progress {
        let (top, top_total, top_rest_size) = {
            let live = self.live.lock().unwrap_or_else(|e| e.into_inner());
            let mut idx: Vec<usize> = (0..live.len()).collect();
            let by_size = |a: &usize, b: &usize| live[*b].size.cmp(&live[*a].size).then(a.cmp(b));
            let take = LIVE_TOP.min(idx.len());
            if take < idx.len() {
                idx.select_nth_unstable_by(take, &by_size);
            }
            let rest: u64 = idx[take..].iter().map(|&i| live[i].size).sum();
            idx.truncate(take);
            idx.sort_unstable_by(&by_size);
            (idx.into_iter().map(|i| live[i].clone()).collect(), live.len(), rest)
        };
        Progress {
            top,
            top_total,
            top_rest_size,
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            errors: self.error_count.load(Ordering::Relaxed),
            current: self.current.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            elapsed: started.elapsed(),
        }
    }

    fn next_task(&self) -> Option<Task> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return None;
            }
            if let Some(t) = q.items.pop() {
                return Some(t);
            }
            if q.pending == 0 {
                return None;
            }
            q = self.work_cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
}

fn worker(sh: &Shared) {
    while let Some(task) = sh.next_task() {
        let _guard = TaskGuard(sh);
        process(sh, task);
    }
}

fn process(sh: &Shared, task: Task) {
    if sh.cancel.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut c) = sh.current.try_lock() {
        c.clear();
        c.push_str(&task.path.to_string_lossy());
    }

    let rd = match fs::read_dir(&task.path) {
        Ok(r) => r,
        Err(e) => {
            sh.record_error(&task.path, &e);
            return;
        }
    };

    let mut found: Vec<Node> = Vec::new();
    let (mut n_files, mut n_dirs, mut n_bytes) = (0u64, 0u64, 0u64);

    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                sh.record_error(&task.path, &e);
                continue;
            }
        };
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                sh.record_error(&entry.path(), &e);
                continue;
            }
        };
        // Enlaces simbólicos y uniones (junctions): se ignoran para no contar dos veces ni entrar en bucles.
        if ft.is_symlink() {
            continue;
        }
        let is_dir = ft.is_dir();
        if !is_dir && !ft.is_file() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                sh.record_error(&entry.path(), &e);
                continue;
            }
        };
        let size = if is_dir { 0 } else { meta.len() };
        if is_dir {
            n_dirs += 1;
        } else {
            n_files += 1;
            n_bytes += size;
        }
        found.push(Node {
            name: entry.file_name().to_string_lossy().into_owned().into_boxed_str(),
            parent: task.id,
            is_dir,
            size,
            files: 0,
            mtime: mtime_of(&meta),
            children: Vec::new(),
            removed: false,
        });
    }

    let mut subdirs: Vec<Task> = Vec::new();
    {
        let is_root = task.id == 0;
        let mut live = sh.live.lock().unwrap_or_else(|e| e.into_inner());
        let mut arena = sh.arena.lock().unwrap_or_else(|e| e.into_inner());
        let mut ids = Vec::with_capacity(found.len());
        for n in found {
            let id = arena.len() as u32;
            // Las hijas de la raíz abren una entrada de primer nivel; el resto hereda la de su ascendiente.
            let slot = if is_root {
                live.push(LiveItem { name: n.name.clone(), is_dir: n.is_dir, size: n.size, files: 0 });
                live.len() - 1
            } else {
                task.live
            };
            if n.is_dir {
                subdirs.push(Task { id, path: task.path.join(&*n.name), live: slot });
            }
            ids.push(id);
            arena.push(n);
        }
        arena[task.id as usize].children = ids;
        if !is_root && task.live != NO_LIVE {
            let l = &mut live[task.live];
            l.size += n_bytes;
            l.files += n_files;
        }
    }
    sh.files.fetch_add(n_files, Ordering::Relaxed);
    sh.dirs.fetch_add(n_dirs, Ordering::Relaxed);
    sh.bytes.fetch_add(n_bytes, Ordering::Relaxed);

    if !subdirs.is_empty() {
        let n = subdirs.len();
        {
            let mut q = sh.queue.lock().unwrap_or_else(|e| e.into_inner());
            q.pending += n; // antes de que el guard descuente esta carpeta: nunca llega a 0 por error
            q.items.extend(subdirs);
        }
        if n == 1 {
            sh.work_cv.notify_one();
        } else {
            sh.work_cv.notify_all();
        }
    }
}

/// Suma tamaños, nº de archivos y fecha hacia arriba. Los hijos siempre tienen
/// un índice mayor que su padre, así que basta recorrer en orden inverso.
fn aggregate(nodes: &mut [Node]) {
    for i in (1..nodes.len()).rev() {
        let (size, files, mtime, parent) = {
            let n = &nodes[i];
            (n.size, if n.is_dir { n.files } else { 1 }, n.mtime, n.parent as usize)
        };
        let p = &mut nodes[parent];
        p.size += size;
        p.files += files;
        if mtime > p.mtime {
            p.mtime = mtime;
        }
    }
}

/// Escanea `root` con `threads` hilos. `on_progress` se llama cada ~150 ms desde
/// el hilo que invoca. Si `cancel` se activa, devuelve lo escaneado hasta ese momento
/// (`cancelled = true`).
pub fn scan<F: FnMut(&Progress)>(
    root: &str,
    threads: usize,
    cancel: Arc<AtomicBool>,
    mut on_progress: F,
) -> Result<Tree, String> {
    let root_path = normalize_root(root)?;
    let meta = fs::metadata(&root_path)
        .map_err(|e| format!("No se puede acceder a «{}»: {}", root_path.display(), e))?;
    if !meta.is_dir() {
        return Err(format!("«{}» no es una carpeta.", root_path.display()));
    }
    let threads = threads.clamp(1, 128);
    let started = Instant::now();

    let root_node = Node {
        name: root_path.to_string_lossy().into_owned().into_boxed_str(),
        parent: NO_PARENT,
        is_dir: true,
        size: 0,
        files: 0,
        mtime: mtime_of(&meta),
        children: Vec::new(),
        removed: false,
    };

    let shared = Shared {
        arena: Mutex::new(vec![root_node]),
        queue: Mutex::new(Queue { items: vec![Task { id: 0, path: root_path, live: NO_LIVE }], pending: 1 }),
        work_cv: Condvar::new(),
        done_cv: Condvar::new(),
        files: AtomicU64::new(0),
        dirs: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        error_count: AtomicU64::new(0),
        errors: Mutex::new(Vec::new()),
        current: Mutex::new(String::new()),
        live: Mutex::new(Vec::new()),
        cancel: cancel.clone(),
    };

    thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| worker(&shared));
        }
        loop {
            let finished = {
                let q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
                if q.pending == 0 {
                    true
                } else {
                    let (q, _) = shared
                        .done_cv
                        .wait_timeout(q, PROGRESS_EVERY)
                        .unwrap_or_else(|e| e.into_inner());
                    q.pending == 0
                }
            };
            on_progress(&shared.snapshot(started));
            if finished {
                break;
            }
            if shared.cancel.load(Ordering::Relaxed) {
                shared.queue.lock().unwrap_or_else(|e| e.into_inner()).items.clear();
                shared.work_cv.notify_all(); // despierta a los hilos dormidos para que salgan
                break;
            }
        }
    });

    let mut nodes = shared.arena.into_inner().unwrap_or_else(|e| e.into_inner());
    aggregate(&mut nodes);
    Ok(Tree {
        nodes,
        errors: shared.errors.into_inner().unwrap_or_else(|e| e.into_inner()),
        error_count: shared.error_count.load(Ordering::Relaxed),
        dirs: shared.dirs.load(Ordering::Relaxed),
        files: shared.files.load(Ordering::Relaxed),
        elapsed: started.elapsed(),
        cancelled: cancel.load(Ordering::Relaxed),
    })
}

// ─────────────────────────── Pruebas ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicUsize;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn tmpdir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("espacio_test_{}_{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn put(p: &Path, n: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::File::create(p).unwrap().write_all(&vec![b'x'; n]).unwrap();
    }

    fn run(root: &Path, threads: usize) -> Tree {
        scan(root.to_str().unwrap(), threads, Arc::new(AtomicBool::new(false)), |_| {}).unwrap()
    }

    fn sample() -> PathBuf {
        let r = tmpdir();
        put(&r.join("a/f1.txt"), 10);
        put(&r.join("a/f2.TXT"), 20);
        put(&r.join("b/c/d/f3.bin"), 300);
        put(&r.join("f4.log"), 4000);
        put(&r.join("noext"), 5);
        fs::create_dir_all(r.join("empty")).unwrap();
        r
    }

    #[test]
    fn totals_and_order() {
        let r = sample();
        let t = run(&r, 4);
        assert_eq!(t.nodes[0].size, 4335);
        assert_eq!(t.files, 5);
        assert_eq!(t.nodes[0].files, 5);
        assert_eq!(t.dirs, 5); // a, b, c, d, empty
        assert_eq!(t.error_count, 0);
        assert!(!t.cancelled);

        let page = t.children_page(0, SortKey::Size, true, 100).unwrap();
        let names: Vec<&str> = page.ids.iter().map(|&i| &*t.nodes[i as usize].name).collect();
        assert_eq!(names, ["f4.log", "b", "a", "noext", "empty"]);
        assert_eq!(page.total, 5);
        assert_eq!(page.rest_count, 0);

        // con tope: lo que queda fuera se resume
        let page = t.children_page(0, SortKey::Size, true, 2).unwrap();
        assert_eq!(page.ids.len(), 2);
        assert_eq!(page.rest_count, 3);
        assert_eq!(page.rest_size, 30 + 5);

        // orden por nombre ascendente
        let page = t.children_page(0, SortKey::Name, false, 100).unwrap();
        let names: Vec<&str> = page.ids.iter().map(|&i| &*t.nodes[i as usize].name).collect();
        assert_eq!(names, ["a", "b", "empty", "f4.log", "noext"]);
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn same_result_with_any_thread_count() {
        let r = tmpdir();
        for d in 0..30 {
            for e in 0..10 {
                for f in 0..8 {
                    put(&r.join(format!("d{d}/e{e}/f{f}.dat")), d * 7 + e * 3 + f + 1);
                }
            }
        }
        let expected = run(&r, 1);
        for th in [2, 8, 64, 128] {
            let t = run(&r, th);
            assert_eq!(t.nodes[0].size, expected.nodes[0].size, "hilos={th}");
            assert_eq!(t.files, 2400);
            assert_eq!(t.dirs, 30 + 300);
            assert_eq!(t.nodes.len(), expected.nodes.len());
        }
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn rejects_bad_roots() {
        let no_cancel = || Arc::new(AtomicBool::new(false));
        assert!(scan("", 2, no_cancel(), |_| {}).is_err());
        assert!(scan("\\\\servidor", 2, no_cancel(), |_| {}).is_err());
        assert!(scan("\\\\servidor\\", 2, no_cancel(), |_| {}).is_err());
        assert!(scan("/ruta/que/no/existe/seguro", 2, no_cancel(), |_| {}).is_err());
        let r = sample();
        assert!(scan(r.join("noext").to_str().unwrap(), 2, no_cancel(), |_| {}).is_err());
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn normalizes_input() {
        assert_eq!(normalize_root("  \"C:\"  ").unwrap(), PathBuf::from("C:\\"));
        assert_eq!(normalize_root("C:\\").unwrap(), PathBuf::from("C:\\"));
        assert_eq!(normalize_root("\\\\srv\\share\\").unwrap(), PathBuf::from("\\\\srv\\share"));
        assert_eq!(normalize_root("/tmp/x/").unwrap(), PathBuf::from("/tmp/x"));
    }

    #[test]
    fn cancel_returns_partial_without_hanging() {
        let r = sample();
        let cancel = Arc::new(AtomicBool::new(true));
        let t = scan(r.to_str().unwrap(), 8, cancel, |_| {}).unwrap();
        assert!(t.cancelled);
        assert!(t.nodes[0].size <= 4335);
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn top_files_extensions_and_paths() {
        let r = sample();
        let t = run(&r, 4);
        let top = t.top_files(3);
        let names: Vec<&str> = top.iter().map(|&i| &*t.nodes[i as usize].name).collect();
        assert_eq!(names, ["f4.log", "f3.bin", "f2.TXT"]);
        assert_eq!(t.full_path(top[1]), r.join("b/c/d/f3.bin"));

        let ext = t.extensions(10);
        let get = |e: &str| ext.iter().find(|x| x.ext == e).map(|x| (x.count, x.size));
        assert_eq!(get("txt"), Some((2, 30))); // .txt y .TXT juntas
        assert_eq!(get("log"), Some((1, 4000)));
        assert_eq!(get(""), Some((1, 5)));
        assert_eq!(ext[0].ext, "log");

        let chain: Vec<&str> = t
            .ancestors(top[1])
            .into_iter()
            .map(|i| &*t.nodes[i as usize].name)
            .collect();
        assert_eq!(chain[1..], ["b", "c", "d", "f3.bin"]);
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn remove_updates_totals_everywhere() {
        let r = sample();
        let mut t = run(&r, 4);
        let b = t.children_page(0, SortKey::Name, false, 100).unwrap().ids[1]; // «b» (b/c/d/f3.bin = 300)
        assert_eq!(&*t.nodes[b as usize].name, "b");

        let rem = t.remove(b).unwrap();
        assert_eq!((rem.size, rem.files, rem.dirs), (300, 1, 3)); // b, c, d
        assert_eq!(t.nodes[0].size, 4035);
        assert_eq!(t.nodes[0].files, 4);
        assert_eq!((t.files, t.dirs), (4, 2));
        assert!(t.remove(b).is_none()); // ya quitado
        assert!(t.remove(0).is_none()); // la raíz nunca

        let names: Vec<&str> = t
            .children_page(0, SortKey::Size, true, 100)
            .unwrap()
            .ids
            .iter()
            .map(|&i| &*t.nodes[i as usize].name)
            .collect();
        assert_eq!(names, ["f4.log", "a", "noext", "empty"]);
        let top = t.top_files(10);
        assert!(top.iter().all(|&i| &*t.nodes[i as usize].name != "f3.bin"));
        assert!(t.extensions(10).iter().all(|e| e.ext != "bin"));
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn graft_reflects_disk_changes() {
        let r = sample(); // a(30) b/c/d(300) f4.log(4000) noext(5) empty
        let mut t = run(&r, 4);
        assert_eq!(t.nodes[0].size, 4335);
        let by_name = |t: &Tree, n: &str| {
            t.children_page(0, SortKey::Name, false, 100)
                .unwrap()
                .ids
                .into_iter()
                .find(|&i| &*t.nodes[i as usize].name == n)
                .unwrap()
        };
        let b = by_name(&t, "b");

        // En disco: se borra lo de dentro de b y se crea otro archivo.
        fs::remove_dir_all(r.join("b/c")).unwrap();
        put(&r.join("b/nuevo.dat"), 70);
        let sub = run(&r.join("b"), 4);
        assert!(t.graft(b, sub));

        assert_eq!(by_name(&t, "b"), b); // el id de la carpeta se conserva
        assert_eq!(t.nodes[b as usize].size, 70);
        assert_eq!(&*t.nodes[b as usize].name, "b"); // nombre corto, no la ruta completa
        assert_eq!(t.nodes[0].size, 4335 - 300 + 70);
        assert_eq!((t.files, t.dirs), (5, 3)); // a, b, empty
        assert_eq!(t.full_path(by_name(&t, "b")), r.join("b"));

        let inside = t.children_page(b, SortKey::Size, true, 10).unwrap();
        assert_eq!(inside.ids.len(), 1);
        assert_eq!(t.full_path(inside.ids[0]), r.join("b/nuevo.dat"));
        assert!(t.top_files(10).iter().all(|&i| &*t.nodes[i as usize].name != "f3.bin"));

        // y también la raíz entera
        put(&r.join("otro.bin"), 1000);
        let all = run(&r, 4);
        let (all_files, all_dirs) = (all.files, all.dirs);
        assert!(t.graft(0, all));
        assert_eq!(t.nodes[0].size, 4335 - 300 + 70 + 1000);
        assert_eq!(t.files, 6);
        assert_eq!(t.dirs, 3); // la raíz no cuenta: a, b, empty
        assert_eq!((t.files, t.dirs), (all_files, all_dirs));
        let _ = fs::remove_dir_all(&r);
    }

    fn q(text: &str, kind: Kind) -> Query {
        Query { text: text.into(), min_size: 0, modified_before: None, exts: vec![], kind }
    }

    fn names(t: &Tree, r: &SearchResult) -> Vec<String> {
        r.ids.iter().map(|&i| t.nodes[i as usize].name.to_string()).collect()
    }

    #[test]
    fn search_filters_combine() {
        let r = sample(); // a/f1.txt(10) a/f2.TXT(20) b/c/d/f3.bin(300) f4.log(4000) noext(5) empty
        let t = run(&r, 4);

        // fragmento, sin distinguir mayúsculas; ordenado por tamaño
        let res = t.search(&q("TXT", Kind::Files), 10);
        assert_eq!(names(&t, &res), ["f2.TXT", "f1.txt"]);
        assert_eq!((res.count, res.total_size), (2, Some(30)));

        // comodines
        assert_eq!(names(&t, &t.search(&q("f?.*", Kind::Files), 10)), ["f4.log", "f3.bin", "f2.TXT", "f1.txt"]);
        assert_eq!(names(&t, &t.search(&q("*.bin", Kind::Files), 10)), ["f3.bin"]);
        assert!(t.search(&q("*.zip", Kind::Files), 10).ids.is_empty());

        // tamaño mínimo y extensiones (varias, sin distinguir mayúsculas)
        let mut f = q("", Kind::Files);
        f.min_size = 100;
        assert_eq!(names(&t, &t.search(&f, 10)), ["f4.log", "f3.bin"]);
        let mut f = q("", Kind::Files);
        f.exts = vec!["txt".into(), "bin".into()];
        assert_eq!(names(&t, &t.search(&f, 10)), ["f3.bin", "f2.TXT", "f1.txt"]);

        // carpetas: sin total (se contaría dos veces)
        let d = t.search(&q("", Kind::Dirs), 10);
        assert_eq!(d.total_size, None);
        assert_eq!(d.count, 5); // a, b, c, d, empty
        assert_eq!(names(&t, &d)[0], "b"); // 300 B: la mayor
        assert_eq!(t.search(&q("", Kind::All), 100).count, 10);

        // límite: se queda con los más grandes pero cuenta todo
        let top = t.search(&q("", Kind::Files), 2);
        assert_eq!(names(&t, &top), ["f4.log", "f3.bin"]);
        assert_eq!(top.count, 5);

        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn search_by_age_and_ignores_removed() {
        let r = sample();
        let old = UNIX_EPOCH + Duration::from_secs(1_000_000);
        fs::File::options().write(true).open(r.join("a/f1.txt")).unwrap().set_modified(old).unwrap();
        let mut t = run(&r, 4);

        let mut f = q("", Kind::Files);
        f.modified_before = Some(2_000_000);
        assert_eq!(names(&t, &t.search(&f, 10)), ["f1.txt"]);
        f.modified_before = Some(i64::MAX);
        assert_eq!(t.search(&f, 10).count, 5);

        let id = t.search(&q("f4", Kind::Files), 1).ids[0];
        t.remove(id);
        assert!(t.search(&q("f4", Kind::Files), 10).ids.is_empty());
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn glob_matching() {
        let g = |p: &str, t: &str| {
            glob(&p.chars().collect::<Vec<_>>(), &t.chars().collect::<Vec<_>>())
        };
        assert!(g("*", "") && g("a*b", "ab") && g("a*b", "axxb") && g("*.txt", "a.txt"));
        assert!(!g("a*b", "abc") && !g("?", "") && !g("a?c", "ac") && g("a?c", "abc"));
        assert!(g("*a*a*", "banana") && !g("*a*a*c", "banana"));
    }

    #[test]
    fn live_view_matches_final_tree() {
        let r = sample();
        let mut last: Option<Progress> = None;
        let t = scan(r.to_str().unwrap(), 4, Arc::new(AtomicBool::new(false)), |p| last = Some(p.clone())).unwrap();
        let p = last.expect("al menos un aviso de progreso");
        assert_eq!(p.top_total, 4 + 1); // a, b, empty, f4.log, noext → 5 entradas
        let listed: u64 = p.top.iter().map(|i| i.size).sum();
        assert_eq!(listed + p.top_rest_size, t.nodes[0].size);
        let by = |n: &str| p.top.iter().find(|i| &*i.name == n).unwrap();
        assert_eq!((by("f4.log").size, by("f4.log").is_dir), (4000, false));
        assert_eq!((by("a").size, by("a").files), (30, 2));
        assert_eq!((by("b").size, by("b").files), (300, 1));
        assert_eq!(&*p.top[0].name, "f4.log"); // ordenado de mayor a menor
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn friendly_errors() {
        assert!(friendly_io(&io::Error::from_raw_os_error(5)).starts_with("Sin permisos"));
        assert!(friendly_io(&io::Error::from_raw_os_error(32)).contains("En uso"));
        assert!(friendly_io(&io::Error::from_raw_os_error(206)).contains("larga"));
        assert_eq!(friendly_io(&io::Error::new(io::ErrorKind::NotFound, "x")), "Ya no existe");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_ignored() {
        let r = tmpdir();
        put(&r.join("a/f.txt"), 100);
        std::os::unix::fs::symlink(&r, r.join("a/bucle")).unwrap();
        std::os::unix::fs::symlink(r.join("a/f.txt"), r.join("enlace.txt")).unwrap();
        let t = run(&r, 4);
        assert_eq!(t.nodes[0].size, 100);
        assert_eq!(t.files, 1);
        let _ = fs::remove_dir_all(&r);
    }
}
