export type Category =
  | "dir"
  | "video"
  | "image"
  | "audio"
  | "doc"
  | "archive"
  | "exe"
  | "data"
  | "other";

export const CATEGORY_INFO: Record<Category, { label: string; color: string }> = {
  dir: { label: "Carpetas", color: "#4f6fa8" },
  video: { label: "Vídeo", color: "#d9693a" },
  image: { label: "Imágenes", color: "#3fa56f" },
  audio: { label: "Audio", color: "#a665c2" },
  doc: { label: "Documentos", color: "#3d7be0" },
  archive: { label: "Comprimidos y copias", color: "#d19a2a" },
  exe: { label: "Programas y librerías", color: "#d1495b" },
  data: { label: "Datos y registros", color: "#1f9fb0" },
  other: { label: "Otros", color: "#7a8499" },
};

const BY_EXT: Record<string, Category> = {};
const add = (cat: Category, exts: string) => exts.split(" ").forEach((e) => (BY_EXT[e] = cat));

add("video", "mp4 mkv avi mov wmv flv webm mpg mpeg m4v ts vob");
add("image", "jpg jpeg png gif bmp tif tiff webp svg ico heic raw psd cr2 nef");
add("audio", "mp3 wav flac aac ogg wma m4a aiff");
add("doc", "pdf doc docx xls xlsx ppt pptx odt ods odp txt rtf csv md msg eml pst ost mdb accdb");
add("archive", "zip rar 7z tar gz bz2 xz iso img bak bkp vhd vhdx vmdk wim cab");
add("exe", "exe dll sys msi bat cmd ps1 jar com ocx");
add("data", "db sqlite sqlite3 log dat bin json xml sql ldf mdf ndf tmp dmp etl");

// Las carpetas se distinguen entre sí con tonos apagados (se reparten por nombre);
// los archivos usan colores más vivos según su tipo.
export const DIR_PALETTE = ["#4f6fa8", "#3f8f8a", "#8068b0", "#a65b79", "#8a7556"];

export function colorOf(name: string, isDir: boolean): string {
  if (!isDir) return CATEGORY_INFO[categoryOf(name, false)].color;
  let h = 5381;
  for (let i = 0; i < name.length; i++) h = ((h << 5) + h + name.charCodeAt(i)) | 0;
  return DIR_PALETTE[Math.abs(h) % DIR_PALETTE.length];
}

export function categoryOf(name: string, isDir: boolean): Category {
  if (isDir) return "dir";
  const p = name.lastIndexOf(".");
  if (p <= 0) return "other";
  return BY_EXT[name.slice(p + 1).toLowerCase()] ?? "other";
}
