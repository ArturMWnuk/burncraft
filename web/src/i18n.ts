// Tłumaczenia interfejsu w formacie Fluent (REQ-03-059): web/locales/<język>.ftl.
// Obsługiwany podzbiór: wiadomości, atrybuty (.title), wiersze kontynuacji,
// zmienne { $nazwa }. Brak klucza w języku → polski → sam klucz.

import en from "../locales/en.ftl?raw";
import pl from "../locales/pl.ftl?raw";

export type Lang = "pl" | "en";

export const LANGS: Record<Lang, string> = { pl: "Polski", en: "English" };

function parse(src: string): Map<string, string> {
  const out = new Map<string, string>();
  let current: string | null = null;
  for (const line of src.split(/\r?\n/)) {
    if (!line.trim() || line.trimStart().startsWith("#")) continue;
    const msg = /^([a-zA-Z][\w-]*)\s*=\s*(.*)$/.exec(line);
    if (msg) {
      current = msg[1];
      out.set(current, msg[2]);
      continue;
    }
    const attr = /^\s+\.([a-zA-Z][\w-]*)\s*=\s*(.*)$/.exec(line);
    if (attr && current) {
      out.set(`${current}.${attr[1]}`, attr[2]);
      continue;
    }
    if (current && /^\s/.test(line)) out.set(current, `${out.get(current)}\n${line.trim()}`);
  }
  return out;
}

const bundles: Record<Lang, Map<string, string>> = { pl: parse(pl), en: parse(en) };
let lang: Lang = "pl";

export function setLang(l: string) {
  lang = l in bundles ? (l as Lang) : "pl";
  document.documentElement.lang = lang;
}

export function getLang(): Lang {
  return lang;
}

/** Tekst wiadomości (`id` albo `id.atrybut`) z podstawionymi zmiennymi. */
export function t(id: string, args: Record<string, string | number> = {}): string {
  const s = bundles[lang].get(id) ?? bundles.pl.get(id) ?? id;
  return s.replace(/\{\s*\$([\w-]+)\s*\}/g, (_, k: string) => String(args[k] ?? ""));
}
