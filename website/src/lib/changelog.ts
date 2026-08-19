// Resolves the Android release history at build time (the site is static
// and ships no client JS). deploy-website.yml re-runs on every release
// publish, so the baked-in changelog tracks the newest tag. Android only:
// it is the release train distributed from GitHub, while iOS notes reach
// their audience through TestFlight and the App Store.
//
// Release bodies are written by release-notes.yml in the site's three
// languages, concatenated under `<!-- inkuna:lang:xx -->` markers; releases
// published before the markers existed are English-only and fall back to
// that English text in every locale.
import { defaultLang, type Lang } from "../i18n/locales";

interface Release {
  tag_name?: string;
  draft?: boolean;
  body?: string;
  published_at?: string;
}

export interface ChangelogEntry {
  /** Marketing version, e.g. "0.6.0". */
  version: string;
  /** Monotonic build number from the tag's `+N` suffix. */
  build: string;
  /** ISO publish timestamp. */
  publishedAt: string;
  /** Notes markdown per language; missing languages fall back to English. */
  notes: Partial<Record<Lang, string>>;
}

const TAG = /^android-v(\d+\.\d+\.\d+)\+(\d+)$/;
const LANG_MARKER = /<!--\s*inkuna:lang:(\w+)\s*-->/g;

/** Splits a release body into its per-language sections; a body without
 *  markers (pre-trilingual releases) is all English. */
export function splitByLanguage(body: string): Partial<Record<Lang, string>> {
  const text = body.replace(/\r\n/g, "\n");
  const markers = [...text.matchAll(LANG_MARKER)];
  if (markers.length === 0) return { en: text.trim() };
  const notes: Partial<Record<Lang, string>> = {};
  markers.forEach((marker, i) => {
    const start = (marker.index ?? 0) + marker[0].length;
    const end = i + 1 < markers.length ? markers[i + 1].index : text.length;
    const section = text.slice(start, end).trim();
    const lang = marker[1] as Lang;
    if (section) notes[lang] = section;
  });
  return notes;
}

export function noteFor(entry: ChangelogEntry, lang: Lang): string {
  return entry.notes[lang] ?? entry.notes[defaultLang] ?? "";
}

export async function changelogEntries(): Promise<ChangelogEntry[]> {
  try {
    const headers: Record<string, string> = {
      Accept: "application/vnd.github+json",
    };
    // CI passes GITHUB_TOKEN to dodge the unauthenticated rate limit.
    if (process.env.GITHUB_TOKEN) {
      headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
    }
    const res = await fetch(
      "https://api.github.com/repos/PythonShe/Inkuna/releases?per_page=100",
      { headers },
    );
    if (!res.ok) throw new Error(`GitHub API ${res.status}`);
    const releases = (await res.json()) as Release[];
    const entries: ChangelogEntry[] = [];
    for (const release of releases) {
      // Newest first; test builds are published as prereleases, only
      // drafts are invisible to the public and skipped.
      if (release.draft) continue;
      const tag = TAG.exec(release.tag_name ?? "");
      if (!tag || !release.body) continue;
      entries.push({
        version: tag[1],
        build: tag[2],
        publishedAt: release.published_at ?? "",
        notes: splitByLanguage(release.body),
      });
    }
    return entries;
  } catch {
    // Offline local builds and API hiccups render an empty history; the
    // page points at the GitHub releases list instead.
    return [];
  }
}

/** The generated notes are a known markdown subset: `###` headings, `-`
 *  bullets, paragraphs. Rendered here at build time so the page ships as
 *  plain HTML; everything is entity-escaped first. */
export function renderNotes(markdown: string): string {
  const escape = (s: string) =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const html: string[] = [];
  let bullets: string[] = [];
  const flush = () => {
    if (bullets.length > 0) {
      html.push(`<ul>${bullets.map((b) => `<li>${b}</li>`).join("")}</ul>`);
      bullets = [];
    }
  };
  for (const raw of markdown.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("- ")) {
      bullets.push(escape(line.slice(2).trim()));
      continue;
    }
    flush();
    if (line === "") continue;
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      // Every note heading renders as h3: the source `###` depth is not a
      // promise, and the page owns h1/h2.
      html.push(`<h3>${escape(heading[2])}</h3>`);
    } else {
      html.push(`<p>${escape(line)}</p>`);
    }
  }
  flush();
  return html.join("\n");
}
