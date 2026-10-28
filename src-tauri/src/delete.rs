//! Borrado definitivo de carpetas grandes.
//!
//! Frente a `fs::remove_dir_all` (secuencial, sin avance y que se detiene en el primer
//! error): este módulo borra en paralelo, informa del progreso, se puede cancelar,
//! sigue adelante aunque algún archivo falle y nunca entra en enlaces simbólicos
//! ni uniones (solo quita el enlace). No depende de Tauri.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::Duration;

const SAMPLE_CAP: usize = 20;
const TICK: Duration = Duration::from_millis(150);

/// Contadores acumulados entre varios elementos borrados seguidos.
#[derive(Default)]
pub struct Counters {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub current: Mutex<String>,
}

pub struct Outcome {
    /// La carpeta/archivo ya no existe.
    pub fully_deleted: bool,
    pub cancelled: bool,
    pub error_count: u64,
    /// Primeros errores (ruta, motivo).
    pub samples: Vec<(String, String)>,
}

struct Q {
    items: Vec<PathBuf>,
    pending: usize,
}

struct Sh<'a> {
    q: Mutex<Q>,
    work_cv: Condvar,
    done_cv: Condvar,
    dirs: Mutex<Vec<PathBuf>>,
    samples: Mutex<Vec<(String, String)>>,
    local_errors: AtomicU64,
    c: &'a Counters,
    cancel: &'a AtomicBool,
}

impl Sh<'_> {
    fn fail(&self, path: &Path, err: &io::Error) {
        self.c.errors.fetch_add(1, Ordering::Relaxed);
        self.local_errors.fetch_add(1, Ordering::Relaxed);
        let mut s = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        if s.len() < SAMPLE_CAP {
            s.push((path.to_string_lossy().into_owned(), crate::scan::friendly_io(err)));
        }
    }

    fn next(&self) -> Option<PathBuf> {
        let mut q = self.q.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return None;
            }
            if let Some(p) = q.items.pop() {
                return Some(p);
            }
            if q.pending == 0 {
                return None;
            }
            q = self.work_cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn finish(&self) {
        let mut q = self.q.lock().unwrap_or_else(|e| e.into_inner());
        q.pending -= 1;
        if q.pending == 0 {
            self.work_cv.notify_all();
            self.done_cv.notify_all();
        }
    }
}

struct Guard<'a, 'b>(&'a Sh<'b>);
impl Drop for Guard<'_, '_> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// Borra un archivo; si falla (típicamente por atributo de solo lectura en Windows)
/// quita ese atributo y lo reintenta. Así no hay llamada extra en el caso normal.
fn remove_file_force(p: &Path) -> io::Result<()> {
    match fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) => {
            #[cfg(windows)]
            {
                if let Ok(m) = fs::symlink_metadata(p) {
                    let mut perm = m.permissions();
                    if perm.readonly() {
                        perm.set_readonly(false);
                        if fs::set_permissions(p, perm).is_ok() {
                            return fs::remove_file(p);
                        }
                    }
                }
            }
            Err(e)
        }
    }
}

/// En recursos de red a veces la carpeta tarda un instante en quedar vacía.
fn remove_dir_retry(p: &Path) -> io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::remove_dir(p) {
            Ok(()) => return Ok(()),
            Err(e) if attempt < 3 && e.kind() == io::ErrorKind::DirectoryNotEmpty => {
                attempt += 1;
                thread::sleep(Duration::from_millis(150));
            }
            Err(e) => return Err(e),
        }
    }
}

fn process(sh: &Sh, dir: PathBuf) {
    if sh.cancel.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut c) = sh.c.current.try_lock() {
        c.clear();
        c.push_str(&dir.to_string_lossy());
    }
    let rd = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) => {
            sh.fail(&dir, &e);
            return;
        }
    };

    let mut sub: Vec<PathBuf> = Vec::new();
    for entry in rd {
        if sh.cancel.load(Ordering::Relaxed) {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                sh.fail(&dir, &e);
                continue;
            }
        };
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                sh.fail(&path, &e);
                continue;
            }
        };
        if ft.is_symlink() {
            // Solo se quita el enlace; nunca se entra en su destino.
            if let Err(e) = fs::remove_file(&path).or_else(|_| fs::remove_dir(&path)) {
                sh.fail(&path, &e);
            }
            continue;
        }
        if ft.is_dir() {
            sub.push(path);
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        match remove_file_force(&path) {
            Ok(()) => {
                sh.c.files.fetch_add(1, Ordering::Relaxed);
                sh.c.bytes.fetch_add(size, Ordering::Relaxed);
            }
            Err(e) => sh.fail(&path, &e),
        }
    }

    if !sub.is_empty() {
        let n = sub.len();
        sh.dirs.lock().unwrap_or_else(|e| e.into_inner()).extend(sub.iter().cloned());
        {
            let mut q = sh.q.lock().unwrap_or_else(|e| e.into_inner());
            q.pending += n;
            q.items.extend(sub);
        }
        if n == 1 {
            sh.work_cv.notify_one();
        } else {
            sh.work_cv.notify_all();
        }
    }
}

fn worker(sh: &Sh) {
    while let Some(dir) = sh.next() {
        let _g = Guard(sh);
        process(sh, dir);
    }
}

fn failed(path: &Path, msg: &str) -> Outcome {
    Outcome {
        fully_deleted: false,
        cancelled: false,
        error_count: 1,
        samples: vec![(path.to_string_lossy().into_owned(), msg.to_string())],
    }
}

/// Borra un elemento escaneado (archivo o carpeta). `tick` se llama unas 7 veces
/// por segundo desde el hilo que invoca, para informar del progreso.
pub fn delete_target<F: FnMut()>(
    path: &Path,
    is_dir: bool,
    threads: usize,
    cancel: &AtomicBool,
    c: &Counters,
    mut tick: F,
) -> Outcome {
    let md = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => return failed(path, &crate::scan::friendly_io(&e)),
    };
    if md.file_type().is_symlink() {
        return failed(path, "ahora es un enlace simbólico; no se toca");
    }
    if md.is_dir() != is_dir {
        return failed(path, "ya no coincide con lo escaneado; vuelve a escanear");
    }

    if !is_dir {
        return match remove_file_force(path) {
            Ok(()) => {
                c.files.fetch_add(1, Ordering::Relaxed);
                c.bytes.fetch_add(md.len(), Ordering::Relaxed);
                Outcome { fully_deleted: true, cancelled: false, error_count: 0, samples: vec![] }
            }
            Err(e) => {
                c.errors.fetch_add(1, Ordering::Relaxed);
                failed(path, &crate::scan::friendly_io(&e))
            }
        };
    }

    let threads = threads.clamp(1, 64);
    let sh = Sh {
        q: Mutex::new(Q { items: vec![path.to_path_buf()], pending: 1 }),
        work_cv: Condvar::new(),
        done_cv: Condvar::new(),
        dirs: Mutex::new(vec![path.to_path_buf()]),
        samples: Mutex::new(Vec::new()),
        local_errors: AtomicU64::new(0),
        c,
        cancel,
    };

    // Fase 1: vaciar carpetas (archivos) en paralelo.
    thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| worker(&sh));
        }
        loop {
            let finished = {
                let q = sh.q.lock().unwrap_or_else(|e| e.into_inner());
                if q.pending == 0 {
                    true
                } else {
                    let (q, _) = sh.done_cv.wait_timeout(q, TICK).unwrap_or_else(|e| e.into_inner());
                    q.pending == 0
                }
            };
            tick();
            if finished {
                break;
            }
            if cancel.load(Ordering::Relaxed) {
                sh.q.lock().unwrap_or_else(|e| e.into_inner()).items.clear();
                sh.work_cv.notify_all();
                break;
            }
        }
    });

    // Fase 2: quitar las carpetas ya vacías, de las más profundas a las más someras.
    if !cancel.load(Ordering::Relaxed) {
        let dirs = std::mem::take(&mut *sh.dirs.lock().unwrap_or_else(|e| e.into_inner()));
        let had_errors = sh.local_errors.load(Ordering::Relaxed) > 0;
        let mut levels: BTreeMap<usize, Vec<PathBuf>> = BTreeMap::new();
        for d in dirs {
            levels.entry(d.components().count()).or_default().push(d);
        }
        for level in levels.values().rev() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let idx = AtomicUsize::new(0);
            let finished = AtomicUsize::new(0);
            let n = threads.min(level.len());
            thread::scope(|s| {
                for _ in 0..n {
                    s.spawn(|| {
                        loop {
                            if cancel.load(Ordering::Relaxed) {
                                break;
                            }
                            let i = idx.fetch_add(1, Ordering::Relaxed);
                            if i >= level.len() {
                                break;
                            }
                            if let Err(e) = remove_dir_retry(&level[i]) {
                                // Si antes falló algún archivo es normal que sus carpetas no se vacíen:
                                // no se duplican errores.
                                if !had_errors {
                                    sh.fail(&level[i], &e);
                                }
                            }
                        }
                        finished.fetch_add(1, Ordering::Release);
                    });
                }
                while finished.load(Ordering::Acquire) < n {
                    tick();
                    thread::sleep(Duration::from_millis(100));
                }
            });
        }
    }

    let samples = std::mem::take(&mut *sh.samples.lock().unwrap_or_else(|e| e.into_inner()));
    Outcome {
        fully_deleted: fs::symlink_metadata(path).is_err(),
        cancelled: cancel.load(Ordering::Relaxed),
        error_count: sh.local_errors.load(Ordering::Relaxed),
        samples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicUsize as Cnt;

    static N: Cnt = Cnt::new(0);

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "espacio_del_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn put(p: &Path, n: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::File::create(p).unwrap().write_all(&vec![b'x'; n]).unwrap();
    }

    #[test]
    fn deletes_big_tree_and_counts() {
        let r = tmp();
        let target = r.join("grande");
        let mut bytes = 0u64;
        for a in 0..20 {
            for b in 0..10 {
                for f in 0..15 {
                    let n = 100 + a + b + f;
                    put(&target.join(format!("a{a}/b{b}/f{f}.dat")), n);
                    bytes += n as u64;
                }
            }
        }
        fs::create_dir_all(target.join("vacia/dentro")).unwrap();
        put(&r.join("vecino.txt"), 5);

        let c = Counters::default();
        let cancel = AtomicBool::new(false);
        let out = delete_target(&target, true, 16, &cancel, &c, || {});
        assert!(out.fully_deleted, "{:?}", out.samples);
        assert_eq!(out.error_count, 0);
        assert!(!target.exists());
        assert!(r.join("vecino.txt").exists());
        assert_eq!(c.files.load(Ordering::Relaxed), 3000);
        assert_eq!(c.bytes.load(Ordering::Relaxed), bytes);
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn deletes_single_file_and_checks_type() {
        let r = tmp();
        put(&r.join("f.txt"), 50);
        fs::create_dir_all(r.join("d")).unwrap();
        let c = Counters::default();
        let cancel = AtomicBool::new(false);

        let wrong = delete_target(&r.join("d"), false, 4, &cancel, &c, || {});
        assert!(!wrong.fully_deleted && r.join("d").exists());
        let missing = delete_target(&r.join("nada"), true, 4, &cancel, &c, || {});
        assert!(!missing.fully_deleted && missing.error_count == 1);

        let ok = delete_target(&r.join("f.txt"), false, 4, &cancel, &c, || {});
        assert!(ok.fully_deleted && !r.join("f.txt").exists());
        assert_eq!(c.bytes.load(Ordering::Relaxed), 50);
        let _ = fs::remove_dir_all(&r);
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_symlinks() {
        let r = tmp();
        let fuera = tmp();
        put(&fuera.join("intocable.txt"), 10);
        let target = r.join("t");
        put(&target.join("x.txt"), 1);
        std::os::unix::fs::symlink(&fuera, target.join("enlace_a_fuera")).unwrap();

        let c = Counters::default();
        let out = delete_target(&target, true, 4, &AtomicBool::new(false), &c, || {});
        assert!(out.fully_deleted, "{:?}", out.samples);
        assert!(fuera.join("intocable.txt").exists(), "se siguió un enlace simbólico");
        let _ = fs::remove_dir_all(&r);
        let _ = fs::remove_dir_all(&fuera);
    }

    #[test]
    fn cancelled_before_start_deletes_nothing_more() {
        let r = tmp();
        put(&r.join("t/a/f.txt"), 10);
        let c = Counters::default();
        let out = delete_target(&r.join("t"), true, 4, &AtomicBool::new(true), &c, || {});
        assert!(out.cancelled && !out.fully_deleted);
        assert!(r.join("t").exists());
        let _ = fs::remove_dir_all(&r);
    }

    #[cfg(unix)]
    #[test]
    fn continues_after_errors_and_reports_them() {
        use std::os::unix::fs::PermissionsExt;
        let r = tmp();
        put(&r.join("t/ok/a.txt"), 1);
        put(&r.join("t/bloqueada/b.txt"), 1);
        put(&r.join("t/ok2/c.txt"), 1);
        // Carpeta sin permiso de escritura: sus archivos no se pueden borrar (si no somos root).
        fs::set_permissions(r.join("t/bloqueada"), fs::Permissions::from_mode(0o555)).unwrap();
        let am_root = fs::File::create(r.join("t/bloqueada/prueba")).is_ok();

        let c = Counters::default();
        let out = delete_target(&r.join("t"), true, 4, &AtomicBool::new(false), &c, || {});
        fs::set_permissions(r.join("t/bloqueada"), fs::Permissions::from_mode(0o755)).ok();
        if am_root {
            assert!(out.fully_deleted); // root ignora los permisos: no se puede simular el fallo
        } else {
            assert!(!out.fully_deleted);
            assert!(out.error_count >= 1 && !out.samples.is_empty());
            assert!(!r.join("t/ok").exists() && !r.join("t/ok2").exists()); // siguió adelante
        }
        let _ = fs::remove_dir_all(&r);
    }
}
