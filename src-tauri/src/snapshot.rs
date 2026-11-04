//! Instantánea del último escaneo de cada ruta, para ver cuánto ha crecido cada carpeta.
//!
//! Solo se guardan los tamaños de las carpetas (no los archivos) en un archivo de texto
//! dentro del perfil del usuario. Se puede desactivar y borrar desde «Ajustes».
//! Formato (texto, una línea por dato, separado por tabuladores):
//!   SGIIPP-SNAP 1 / root\t… / time\t… / rootsize\t… / cutoff\t… / luego «size\truta-relativa»

use crate::scan::{Tree, NO_PARENT};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MAX_DIRS: usize = 40_000;
const MAGIC: &str = "SGIIPP-SNAP 1";

pub struct Snapshot {
    pub root: String,
    /// Segundos Unix en que se hizo el escaneo.
    pub time: i64,
    pub root_size: u64,
    /// Si se recortó la lista: tamaño de la carpeta más pequeña guardada (las menores no constan).
    pub cutoff: u64,
    pub dirs: HashMap<String, u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Delta {
    /// Tamaño que tenía en el escaneo anterior.
    pub prev: Option<u64>,
    /// No existía antes (y su tamaño sí habría quedado registrado).
    pub is_new: bool,
}

/// Clave de una ruta raíz: sin mayúsculas, con `\` y sin barra final.
pub fn norm_key(root: &str) -> String {
    let s: String = root.chars().flat_map(char::to_lowercase).map(|c| if c == '/' { '\\' } else { c }).collect();
    s.trim_end_matches('\\').to_string()
}

/// Ruta de una carpeta relativa a la raíz del escaneo (vacía para la raíz), en minúsculas.
pub fn rel_key(tree: &Tree, id: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = id;
    while tree.nodes[cur as usize].parent != NO_PARENT {
        parts.push(tree.nodes[cur as usize].name.chars().flat_map(char::to_lowercase).collect());
        cur = tree.nodes[cur as usize].parent;
    }
    parts.reverse();
    parts.join("\\")
}

pub fn build(tree: &Tree, now: i64) -> Snapshot {
    let mut dirs: Vec<(u64, u32)> = tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| n.is_dir && !n.removed && n.parent != NO_PARENT && *i != 0)
        .map(|(i, n)| (n.size, i as u32))
        .collect();
    let mut cutoff = 0;
    if dirs.len() > MAX_DIRS {
        dirs.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        dirs.truncate(MAX_DIRS);
        cutoff = dirs.last().map_or(0, |d| d.0);
    }
    Snapshot {
        root: norm_key(&tree.nodes[0].name),
        time: now,
        root_size: tree.nodes[0].size,
        cutoff,
        dirs: dirs.into_iter().map(|(s, i)| (rel_key(tree, i), s)).collect(),
    }
}

/// Compara una carpeta del árbol actual con la instantánea anterior.
pub fn compare(prev: &Snapshot, tree: &Tree, id: u32) -> Delta {
    let n = &tree.nodes[id as usize];
    if !n.is_dir {
        return Delta { prev: None, is_new: false };
    }
    if n.parent == NO_PARENT {
        return Delta { prev: Some(prev.root_size), is_new: false };
    }
    match prev.dirs.get(&rel_key(tree, id)) {
        Some(&s) => Delta { prev: Some(s), is_new: false },
        // No consta: o es nueva o es tan pequeña que se recortó de la lista.
        None => Delta { prev: None, is_new: prev.cutoff == 0 || n.size >= prev.cutoff },
    }
}

// ───────────────────────────── Disco ─────────────────────────────

/// Carpeta donde se guardan las instantáneas (perfil del usuario).
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("SGIIPP-DISKANALIZER").join("escaneos")
}

fn fnv(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn file_for(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("{:016x}.snap", fnv(&norm_key(root))))
}

pub fn save(dir: &Path, snap: &Snapshot) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let target = file_for(dir, &snap.root);
    let tmp = target.with_extension("tmp");
    {
        let mut f = io::BufWriter::new(fs::File::create(&tmp)?);
        writeln!(f, "{MAGIC}")?;
        writeln!(f, "root\t{}", snap.root)?;
        writeln!(f, "time\t{}", snap.time)?;
        writeln!(f, "rootsize\t{}", snap.root_size)?;
        writeln!(f, "cutoff\t{}", snap.cutoff)?;
        for (path, size) in &snap.dirs {
            writeln!(f, "{size}\t{path}")?;
        }
        f.flush()?;
    }
    fs::rename(&tmp, &target)
}

pub fn load(dir: &Path, root: &str) -> Option<Snapshot> {
    let text = fs::read_to_string(file_for(dir, root)).ok()?;
    let mut lines = text.lines();
    if lines.next()? != MAGIC {
        return None;
    }
    let mut s = Snapshot { root: String::new(), time: 0, root_size: 0, cutoff: 0, dirs: HashMap::new() };
    let mut header = 0;
    for l in lines {
        let (k, v) = l.split_once('\t')?;
        if header < 4 {
            match k {
                "root" => s.root = v.to_string(),
                "time" => s.time = v.parse().ok()?,
                "rootsize" => s.root_size = v.parse().ok()?,
                "cutoff" => s.cutoff = v.parse().ok()?,
                _ => return None,
            }
            header += 1;
        } else {
            s.dirs.insert(v.to_string(), k.parse().ok()?);
        }
    }
    (header == 4 && s.root == norm_key(root)).then_some(s)
}

/// Borra todas las instantáneas. Devuelve cuántas había.
pub fn clear(dir: &Path) -> usize {
    let mut n = 0;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "snap" || x == "tmp") && fs::remove_file(&p).is_ok() {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn scan_dir(p: &Path) -> Tree {
        scan::scan(p.to_str().unwrap(), 2, Arc::new(AtomicBool::new(false)), |_| {}).unwrap()
    }

    fn child(t: &Tree, name: &str) -> u32 {
        t.nodes[0].children.iter().copied().find(|&c| &*t.nodes[c as usize].name == name).unwrap()
    }

    #[test]
    fn keys_ignore_case_and_trailing_separators() {
        assert_eq!(norm_key("\\\\SRV\\Datos\\"), "\\\\srv\\datos");
        assert_eq!(norm_key("C:/Users/"), "c:\\users");
    }

    #[test]
    fn compares_growth_new_and_untracked_folders() {
        let base = std::env::temp_dir().join(format!("sgiipp_snap_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("datos");
        fs::create_dir_all(root.join("A/sub")).unwrap();
        fs::create_dir_all(root.join("B")).unwrap();
        fs::write(root.join("A/sub/x.bin"), vec![1u8; 1000]).unwrap();
        fs::write(root.join("B/y.bin"), vec![1u8; 500]).unwrap();
        let t1 = scan_dir(&root);
        let snap = build(&t1, 1_700_000_000);
        assert_eq!(snap.dirs.get("a\\sub"), Some(&1000));
        assert_eq!(snap.dirs.get("b"), Some(&500));

        // Guardar y cargar
        let store = base.join("store");
        save(&store, &snap).unwrap();
        let back = load(&store, &root.to_string_lossy().to_uppercase()).expect("la clave no distingue mayúsculas");
        assert_eq!(back.time, 1_700_000_000);
        assert_eq!(back.dirs.len(), 3);
        assert!(load(&store, "otra\\ruta").is_none());

        // Cambios: A crece, B desaparece, C es nueva
        fs::write(root.join("A/sub/z.bin"), vec![1u8; 250]).unwrap();
        fs::remove_dir_all(root.join("B")).unwrap();
        fs::create_dir_all(root.join("C")).unwrap();
        fs::write(root.join("C/n.bin"), vec![1u8; 10]).unwrap();
        let t2 = scan_dir(&root);
        let a = child(&t2, "A");
        let c = child(&t2, "C");
        assert_eq!(compare(&back, &t2, a), Delta { prev: Some(1000), is_new: false });
        assert_eq!(t2.nodes[a as usize].size, 1250);
        assert_eq!(compare(&back, &t2, c), Delta { prev: None, is_new: true });
        assert_eq!(compare(&back, &t2, 0).prev, Some(1500));
        // Un archivo nunca tiene comparación
        let f = t2.nodes[a as usize].children.iter().copied().find(|&i| !t2.nodes[i as usize].is_dir);
        assert!(f.is_none(), "A solo contiene la subcarpeta");

        // Lista recortada: lo pequeño que no consta no se marca como nuevo
        let mut cut = Snapshot { root: back.root.clone(), time: 0, root_size: 1500, cutoff: 400, dirs: HashMap::new() };
        cut.dirs.insert("a".into(), 1000);
        assert_eq!(compare(&cut, &t2, c), Delta { prev: None, is_new: false }, "10 B < 400 B: puede haberse recortado");

        assert_eq!(clear(&store), 1);
        assert!(load(&store, &root.to_string_lossy()).is_none());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn corrupt_files_are_ignored() {
        let base = std::env::temp_dir().join(format!("sgiipp_snap_bad_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let f = file_for(&base, "x");
        fs::write(&f, "basura\n").unwrap();
        assert!(load(&base, "x").is_none());
        fs::write(&f, format!("{MAGIC}\nroot\tx\ntime\t5\nrootsize\t1\ncutoff\t0\nabc\tfoo\n")).unwrap();
        assert!(load(&base, "x").is_none(), "tamaño no numérico");
        let _ = fs::remove_dir_all(&base);
    }
}
