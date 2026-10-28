# Historial de versiones

Formato MAYOR.MINOR.PATCH: MAYOR si cambia algo incompatible, MINOR si se añade una función, PATCH si solo se corrige un error.

## 1.4.1

- Nuevo diálogo «Acerca de» con créditos (Servicios de Informática, CIS Guillermo Miranda, Murcia, SGIIPP). También desde el pie.

## 1.4.0

- Columna «%» con barra de color según el peso (azul, ámbar desde el 15 %, rojo desde el 40 %).
- Resaltado enlazado: la fila sobre la que está el ratón se marca en el mapa y viceversa.
- Tema automático / claro / oscuro con botón en la barra (se recuerda por usuario).
- Pantalla de inicio con tarjetas de las unidades (espacio libre y ocupación) y últimas rutas; un clic escanea.
- Manual actualizado.

## 1.3.3

- Manual actualizado: botones Ajustes y Manual, y problema frecuente sobre el nombre del centro en los informes.

## 1.3.2

- La aplicación pasa a llamarse SGIIPP-DISKANALIZER (título, manual, informes y ejecutable `sgiipp-diskanalizer.exe`).

## 1.3.1

- El ejecutable pasa a llamarse `spaceanalizer.exe`.

## 1.3.0

- Manual de ayuda integrado y accesible (botón «Manual» o tecla F1): índice, diálogo modal con foco y teclado, Esc para cerrar.
- Nuevo diálogo «Ajustes» con el nombre del centro y del servicio para los informes PDF (banda, cabecera, pie y autor). Se guarda por usuario.
- Valores por defecto configurables con un archivo `espacio.ini` junto al .exe (`centro=...`, `servicio=...`, UTF-8). Prioridad: Ajustes > espacio.ini > valor por defecto.

## 1.2.0 — 2026-10-21

- Exportar informes (botón «Exportar…» en la carpeta actual y en los resultados de búsqueda):
  - **PDF** con el estilo de los informes del centro: banda azul con título y ruta, tarjetas de resumen, tabla con barras de peso, total y numeración de páginas. Hasta 3.000 filas; el resto se resume en una línea.
  - **CSV para Excel**: UTF-8 con BOM, separador `;`, decimales con coma y fechas dd/mm/aaaa; incluye la ruta completa y el tamaño exacto en bytes (hasta 50.000 filas).
  - Aviso al terminar con enlace «Mostrar en el Explorador».
- Confirmación reforzada al borrar: si lo seleccionado supera 5 GB (1 GB en unidades de red) hay que escribir ELIMINAR para habilitar el botón.
- Selección como en el Explorador: Shift+clic marca un rango y Ctrl+clic suma o quita elementos, en la tabla de carpetas y en los resultados de búsqueda.

## 1.1.0 — 2026-10-20

- Vista en vivo durante el escaneo: la tabla y el treemap del primer nivel se van llenando mientras se lee el disco, con los tamaños creciendo. Ya no hace falta esperar al final para ver dónde está el espacio.
- Nueva pestaña «Buscar» (Ctrl+F): por nombre (fragmento o comodines `*` y `?`), tipo (archivos, carpetas o ambos), tamaño mínimo, tiempo sin modificar (1 mes a 5 años) y extensiones. Se puede seleccionar y eliminar directamente desde los resultados.
- Menú contextual (clic derecho) en tablas y treemap: abrir carpeta, mostrar en el Explorador, copiar ruta, seleccionar y eliminar.
- Atajos: F5 actualiza la carpeta (ya no recarga la ventana), Ctrl+F busca, Ctrl+A selecciona todo lo visible, Supr elimina lo seleccionado, Esc quita la selección, Retroceso o Alt+← sube.
- El treemap tiene tooltip propio con ruta completa, tamaño, porcentaje, número de archivos y fecha.
- Motivos de error en español claro (sin permisos, en uso por otro programa, ruta demasiado larga, se perdió la conexión de red…) en el escaneo y en el borrado.

## 1.0.1 — 2026-10-20

- Corregido: tras borrar, la lista de carpetas y el treemap no se actualizaban. Ahora, después de cualquier borrado (completo, parcial o detenido) se vuelve a leer del disco lo afectado y la vista refleja lo que realmente queda.
- Borrado de carpetas grandes rehecho: en paralelo, con progreso en vivo (archivos, tamaño liberado, tiempo, ruta actual), botón «Detener», y sigue adelante si un archivo falla (indica cuáles). Quita el atributo de solo lectura al borrar.
- Nuevo botón «Actualizar» (junto a las migas de pan) para volver a leer del disco la carpeta actual sin reescanear todo.

## 1.0.0 — 2026-10-20

- Escáner paralelo para unidades locales y de red (`\\servidor\recurso`), con progreso en vivo y opción de detener.
- Vistas: carpetas con treemap, archivos más grandes, espacio por extensión y errores de acceso.
- Selección múltiple y eliminación definitiva con confirmación.
- Números de versión visibles en la aplicación.
