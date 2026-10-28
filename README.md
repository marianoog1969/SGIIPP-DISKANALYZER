# SGIIPP-DISKANALIZER

Analizador de espacio en disco (estilo TreeSize) para unidades locales y de red.
Tauri 2 + Rust + React/TypeScript.

Servicios de Informática · CIS Guillermo Miranda · Murcia

## Compilar

Requisitos: Rust ≥ 1.90 (`rustup update`), Node 18+ y WebView2 (ya viene con Windows 10/11).

```
npm install
npm run tauri dev                          # desarrollo
npm run tauri build -- --no-bundle         # build de producción sin instalador
```

El ejecutable queda en `src-tauri\target\release\sgiipp-diskanalizer.exe`.

## Versiones

Formato MAYOR.MINOR.PATCH (la versión actual sale en el pie de la aplicación y en el nombre del zip). Para subir de versión:

```
npm run version:set -- 1.1.0
```

Actualiza `package.json`, `package-lock.json`, `Cargo.toml` y `Cargo.lock`; Tauri toma la versión de `Cargo.toml`. Anota los cambios en `CHANGELOG.md`.

## Uso

- Escribe una ruta (`\\servidor\recurso`, `Z:\`, `C:\Datos`), pulsa una unidad o «Examinar…», y «Escanear».
- Se usan las credenciales de tu sesión de Windows. Ejecútalo como usuario normal, **no como administrador**: un proceso elevado no ve las unidades de red mapeadas de la sesión normal.
- **Hilos**: en red, 16–32 suele ir bien; en un disco mecánico local, 4–8.
- «Detener» no descarta nada: muestra lo escaneado hasta ese momento (marcado como parcial).
- Clic en una carpeta (tabla o treemap) para entrar; Retroceso o la flecha para subir.
- **Eliminar**: marca casillas en la tabla de carpetas y pulsa «Eliminar…». Pide confirmación y el borrado es **definitivo (sin papelera)**. Si marcas una carpeta y algo de dentro, solo se borra la carpeta. La raíz del escaneo y las raíces de unidad no se pueden borrar. Muestra progreso y se puede detener; si algún archivo no se puede borrar, sigue con el resto y te dice cuáles. Al terminar se vuelve a leer del disco lo afectado, así que la lista siempre refleja lo que queda. El botón ⟳ de las migas de pan actualiza la carpeta actual desde el disco.
- **Vista en vivo**: mientras escanea se ve el primer nivel con los tamaños creciendo (tabla y treemap). «Detener» conserva lo escaneado.
- **Buscar** (Ctrl+F): nombre (fragmento o comodines `*` y `?`, por ejemplo `*.bak`), tipo, tamaño mínimo, tiempo sin modificar y extensiones (`iso, zip, bak`). Los resultados se pueden seleccionar y eliminar. Con «Carpetas» o «Ambos» no se muestra el total (se contaría dos veces).
- **Exportar…** (junto a las migas de pan y en Buscar): informe **PDF** (estilo del centro) o **CSV para Excel** de la carpeta actual o de los resultados de la búsqueda. El PDF lleva hasta 3.000 filas y resume el resto; el CSV, hasta 50.000.
- **Borrado grande**: si lo seleccionado pesa más de 5 GB (1 GB en red), hay que escribir ELIMINAR para confirmar.
- **Selección múltiple**: casillas, Shift+clic (rango) y Ctrl+clic (suma o quita).
- **Clic derecho** en una fila o en el treemap: abrir, mostrar en el Explorador, copiar ruta, seleccionar, eliminar.
- **Atajos**: F5 actualizar la carpeta · Ctrl+F buscar · Ctrl+A seleccionar todo lo visible · Supr eliminar la selección · Esc quitar selección o cerrar menú · Retroceso o Alt+← subir de carpeta.
- Pestañas: carpetas, buscar, archivos más grandes, espacio por extensión y errores (carpetas sin permiso).

## Cómo funciona

- `src-tauri/src/scan.rs`: escáner paralelo (pool de hilos, una enumeración por carpeta, sin `stat` extra por archivo en Windows). No depende de Tauri y trae pruebas (`cd src-tauri && cargo test`).
- El árbol completo se queda en memoria en Rust (≈100 bytes por archivo); la interfaz solo pide un nivel cada vez.
- Se ignoran enlaces simbólicos y uniones (junctions) para no contar dos veces ni entrar en bucles.
- Tamaño = tamaño lógico del archivo, no el ocupado en disco (calcular este último exige una llamada de red más por archivo).


## Nombre del centro en los informes

Desde «Ajustes» o con un archivo `espacio.ini` junto al ejecutable:

```
centro=Nombre del centro
servicio=Nombre del servicio
```

Ayuda integrada: botón «Manual» o tecla F1.
