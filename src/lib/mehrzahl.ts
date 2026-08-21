import { spracheJetzt } from "./i18n";

// counted nouns in the right inflection.
//
// german and english know two forms, russian four, arabic six, and chinese
// gets by with one. handing over two words, one and many, therefore does for
// half the list only: besides "трек" and "треков" russian needs "трека" for 2
// to 4, and the app therefore read "7 Треки" instead of "7 треков".
//
// `Intl.PluralRules` knows these rules already, it says for every number and
// language which of the categories `zero`, `one`, `two`, `few`, `many`,
// `other` applies. only the words for them stand here.
//
// the lookup runs in order: matching category, then `other`, then english,
// last the german keyword. a missing form therefore falls back to a usable
// one instead of to a code
type Formen = Partial<Record<Intl.LDMLPluralRule, string>>;

/**
 * keyed by the german keyword, the same pattern as the text table.
 *
 * lowercase where the language demands it: "42 tracks", not "42 Tracks". only
 * german capitalises nouns, in the middle of a sentence too.
 */
const FORMEN: Record<string, Record<string, Formen>> = {
  Titel: {
    de: { one: "Titel", other: "Titel" },
    en: { one: "track", other: "tracks" },
    es: { one: "canción", other: "canciones" },
    fr: { one: "titre", other: "titres" },
    ru: { one: "трек", few: "трека", many: "треков", other: "трека" },
    ar: {
      zero: "مقطع",
      one: "مقطع",
      two: "مقطعان",
      few: "مقاطع",
      many: "مقطعًا",
      other: "مقطع",
    },
    zh: { other: "首曲目" },
  },
  Künstler: {
    de: { one: "Künstler", other: "Künstler" },
    en: { one: "artist", other: "artists" },
    es: { one: "artista", other: "artistas" },
    fr: { one: "artiste", other: "artistes" },
    ru: {
      one: "исполнитель",
      few: "исполнителя",
      many: "исполнителей",
      other: "исполнителя",
    },
    ar: {
      zero: "فنان",
      one: "فنان",
      two: "فنانان",
      few: "فنانين",
      many: "فنانًا",
      other: "فنان",
    },
    zh: { other: "位艺术家" },
  },
  Release: {
    de: { one: "Release", other: "Releases" },
    en: { one: "release", other: "releases" },
    es: { one: "lanzamiento", other: "lanzamientos" },
    fr: { one: "sortie", other: "sorties" },
    ru: { one: "релиз", few: "релиза", many: "релизов", other: "релиза" },
    ar: {
      zero: "إصدار",
      one: "إصدار",
      two: "إصداران",
      few: "إصدارات",
      many: "إصدارًا",
      other: "إصدار",
    },
    zh: { other: "张发行" },
  },
  Playlist: {
    de: { one: "Playlist", other: "Playlists" },
    en: { one: "playlist", other: "playlists" },
    es: { one: "lista", other: "listas" },
    fr: { one: "playlist", other: "playlists" },
    ru: {
      one: "плейлист",
      few: "плейлиста",
      many: "плейлистов",
      other: "плейлиста",
    },
    ar: {
      zero: "قائمة تشغيل",
      one: "قائمة تشغيل",
      two: "قائمتا تشغيل",
      few: "قوائم تشغيل",
      many: "قائمة تشغيل",
      other: "قائمة تشغيل",
    },
    zh: { other: "个播放列表" },
  },
  Eintrag: {
    de: { one: "Eintrag", other: "Einträge" },
    en: { one: "entry", other: "entries" },
    es: { one: "entrada", other: "entradas" },
    fr: { one: "entrée", other: "entrées" },
    ru: { one: "запись", few: "записи", many: "записей", other: "записи" },
    ar: {
      zero: "إدخال",
      one: "إدخال",
      two: "إدخالان",
      few: "إدخالات",
      many: "إدخالًا",
      other: "إدخال",
    },
    zh: { other: "个条目" },
  },
  Paket: {
    de: { one: "Paket", other: "Pakete" },
    en: { one: "package", other: "packages" },
    es: { one: "paquete", other: "paquetes" },
    fr: { one: "paquet", other: "paquets" },
    ru: { one: "пакет", few: "пакета", many: "пакетов", other: "пакета" },
    ar: {
      zero: "حزمة",
      one: "حزمة",
      two: "حزمتان",
      few: "حزم",
      many: "حزمة",
      other: "حزمة",
    },
    zh: { other: "个软件包" },
  },
  Mal: {
    de: { one: "Mal", other: "Mal" },
    en: { one: "play", other: "plays" },
    es: { one: "vez", other: "veces" },
    fr: { one: "fois", other: "fois" },
    ru: { one: "раз", few: "раза", many: "раз", other: "раза" },
    ar: {
      zero: "مرة",
      one: "مرة",
      two: "مرتان",
      few: "مرات",
      many: "مرة",
      other: "مرة",
    },
    zh: { other: "次" },
  },
  Wiedergabe: {
    de: { one: "Wiedergabe", other: "Wiedergaben" },
    en: { one: "play", other: "plays" },
    es: { one: "reproducción", other: "reproducciones" },
    fr: { one: "écoute", other: "écoutes" },
    ru: {
      one: "прослушивание",
      few: "прослушивания",
      many: "прослушиваний",
      other: "прослушивания",
    },
    ar: {
      zero: "تشغيل",
      one: "تشغيل",
      two: "تشغيلان",
      few: "عمليات تشغيل",
      many: "تشغيلًا",
      other: "تشغيل",
    },
    zh: { other: "次播放" },
  },
  "Titel ausgewählt": {
    de: { one: "Titel ausgewählt", other: "Titel ausgewählt" },
    en: { one: "track selected", other: "tracks selected" },
    es: { one: "canción seleccionada", other: "canciones seleccionadas" },
    fr: { one: "titre sélectionné", other: "titres sélectionnés" },
    ru: {
      one: "трек выбран",
      few: "трека выбрано",
      many: "треков выбрано",
      other: "трека выбрано",
    },
    ar: { few: "مقاطع محددة", other: "مقطع محدد" },
    zh: { other: "首曲目已选择" },
  },
  "Titel importiert": {
    de: { one: "Titel importiert", other: "Titel importiert" },
    en: { one: "track imported", other: "tracks imported" },
    es: { one: "canción importada", other: "canciones importadas" },
    fr: { one: "titre importé", other: "titres importés" },
    ru: {
      one: "трек импортирован",
      few: "трека импортировано",
      many: "треков импортировано",
      other: "трека импортировано",
    },
    ar: { few: "مقاطع مستوردة", other: "مقطع مستورد" },
    zh: { other: "首曲目已导入" },
  },
  "Titel nachgetragen": {
    de: { one: "Titel nachgetragen", other: "Titel nachgetragen" },
    en: { one: "track added", other: "tracks added" },
    es: { one: "canción añadida", other: "canciones añadidas" },
    fr: { one: "titre ajouté", other: "titres ajoutés" },
    ru: {
      one: "трек добавлен",
      few: "трека добавлено",
      many: "треков добавлено",
      other: "трека добавлено",
    },
    ar: { few: "مقاطع مضافة", other: "مقطع مضاف" },
    zh: { other: "首曲目已补充" },
  },
  "Titel ohne Datei": {
    de: { one: "Titel ohne Datei", other: "Titel ohne Datei" },
    en: { one: "track without file", other: "tracks without file" },
    es: { one: "canción sin archivo", other: "canciones sin archivo" },
    fr: { one: "titre sans fichier", other: "titres sans fichier" },
    ru: {
      one: "трек без файла",
      few: "трека без файла",
      many: "треков без файла",
      other: "трека без файла",
    },
    ar: { few: "مقاطع بلا ملف", other: "مقطع بلا ملف" },
    zh: { other: "首曲目缺少文件" },
  },
  "Datei liegt im Ordner": {
    de: { one: "Datei liegt im Ordner", other: "Dateien liegen im Ordner" },
    en: { one: "file lies in the folder", other: "files lie in the folder" },
    es: {
      one: "archivo está en la carpeta",
      other: "archivos están en la carpeta",
    },
    fr: {
      one: "fichier se trouve dans le dossier",
      other: "fichiers se trouvent dans le dossier",
    },
    ru: {
      one: "файл лежит в папке",
      few: "файла лежат в папке",
      many: "файлов лежат в папке",
      other: "файла лежат в папке",
    },
    ar: { few: "ملفات في المجلد", other: "ملف في المجلد" },
    zh: { other: "个文件位于文件夹中" },
  },
  "Künstler in deiner Bibliothek": {
    de: {
      one: "Künstler in deiner Bibliothek",
      other: "Künstler in deiner Bibliothek",
    },
    en: { one: "artist in your library", other: "artists in your library" },
    es: { one: "artista en tu biblioteca", other: "artistas en tu biblioteca" },
    fr: {
      one: "artiste dans ta bibliothèque",
      other: "artistes dans ta bibliothèque",
    },
    ru: {
      one: "исполнитель в твоей библиотеке",
      few: "исполнителя в твоей библиотеке",
      many: "исполнителей в твоей библиотеке",
      other: "исполнителя в твоей библиотеке",
    },
    ar: { few: "فنانين في مكتبتك", other: "فنان في مكتبتك" },
    zh: { other: "位艺术家在你的音乐库中" },
  },
  "Eintrag entfernt": {
    de: { one: "Eintrag entfernt", other: "Einträge entfernt" },
    en: { one: "entry removed", other: "entries removed" },
    es: { one: "entrada eliminada", other: "entradas eliminadas" },
    fr: { one: "entrée supprimée", other: "entrées supprimées" },
    ru: {
      one: "запись удалена",
      few: "записи удалено",
      many: "записей удалено",
      other: "записи удалено",
    },
    ar: { few: "إدخالات محذوفة", other: "إدخال محذوف" },
    zh: { other: "个条目已移除" },
  },
  "Eintrag zurückgeholt": {
    de: { one: "Eintrag zurückgeholt", other: "Einträge zurückgeholt" },
    en: { one: "entry restored", other: "entries restored" },
    es: { one: "entrada restaurada", other: "entradas restauradas" },
    fr: { one: "entrée restaurée", other: "entrées restaurées" },
    ru: {
      one: "запись восстановлена",
      few: "записи восстановлено",
      many: "записей восстановлено",
      other: "записи восстановлено",
    },
    ar: { few: "إدخالات مستعادة", other: "إدخال مستعاد" },
    zh: { other: "个条目已恢复" },
  },
};

/** test helper: whether the keyword is in the table, and if so in this language */
export function hatFormen(stichwort: string, sprache?: string): boolean {
  const eintrag = FORMEN[stichwort];
  if (!eintrag) return false;
  return sprache === undefined || eintrag[sprache] !== undefined;
}

/** every keyword, for the coverage test */
export function stichwoerter(): string[] {
  return Object.keys(FORMEN);
}

/**
 * the form of a keyword matching the count, without the number itself.
 *
 * creating `Intl.PluralRules` with an unknown language throws, hence the
 * fallback to english should a language arrive before its forms do.
 */
export function mehrzahl(anzahl: number, stichwort: string): string {
  const sprache = spracheJetzt();
  const formen = FORMEN[stichwort]?.[sprache] ?? FORMEN[stichwort]?.en;
  if (!formen) return stichwort;

  let kategorie: Intl.LDMLPluralRule = "other";
  try {
    kategorie = new Intl.PluralRules(sprache).select(anzahl);
  } catch {
    kategorie = anzahl === 1 ? "one" : "other";
  }
  return formen[kategorie] ?? formen.other ?? formen.one ?? stichwort;
}
