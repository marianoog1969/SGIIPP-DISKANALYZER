//! Lista de unidades disponibles (solo en Windows; en otros sistemas devuelve «/»).
//! Usa `GetLogicalDrives`, que no sondea las unidades de red desconectadas
//! (no se cuelga si hay una unidad mapeada caída).

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Drive {
    pub path: String,
    /// local | red | extraible | cd | ram | otro
    pub kind: String,
}

#[cfg(windows)]
pub fn list() -> Vec<Drive> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
    }

    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let path = format!("{}:\\", (b'A' + i as u8) as char);
        let wide: Vec<u16> = std::ffi::OsStr::new(&path).encode_wide().chain(Some(0)).collect();
        let kind = match unsafe { GetDriveTypeW(wide.as_ptr()) } {
            2 => "extraible",
            3 => "local",
            4 => "red",
            5 => "cd",
            6 => "ram",
            _ => "otro",
        };
        out.push(Drive { path, kind: kind.to_string() });
    }
    out
}

#[cfg(not(windows))]
pub fn list() -> Vec<Drive> {
    vec![Drive { path: "/".into(), kind: "local".into() }]
}

#[derive(Serialize, Clone)]
pub struct Space {
    pub total: u64,
    pub free: u64,
}

/// Capacidad y espacio libre de una unidad o ruta (Windows). `None` si no se puede consultar.
#[cfg(windows)]
pub fn space(path: &str) -> Option<Space> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    }

    let wide: Vec<u16> = std::ffi::OsStr::new(path).encode_wide().chain(Some(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    if ok == 0 || total == 0 {
        None
    } else {
        Some(Space { total, free: avail })
    }
}

#[cfg(not(windows))]
pub fn space(_path: &str) -> Option<Space> {
    None
}
