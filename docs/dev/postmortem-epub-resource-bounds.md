# Postmortem — EPUB resource bounds — 2026-08-15

## Summary

One review comment on the core PR — "unbounded EPUB entry decompression,
add per-entry and aggregate decompression budgets" — took **eight rounds**
to close. Every round added a correct, well-reasoned cap. Every round was
then bypassed through a door nobody had looked at, and every bypass was
found by a *different* reviewer (an adversarial panel lens, then three
separate QC rounds) — never by whoever wrote the previous fix.

The bug class: **a small crafted EPUB that a bounded parser turns into an
unbounded amount of memory or of permanent database rows.** It matters here
more than it would in a server parser:

- Inkuna is a local, offline reader. Every byte is spent on the user's
  phone, and there is no operator watching a dashboard.
- iOS jetsam kills a foreground app at a few hundred MB. An RSS spike is
  not a slow request; it is the app disappearing mid-import.
- The library DB is the user's own irreplaceable data. An import that
  returns `Ok` while writing 600 MB of junk rows into it is worse than one
  that fails, because nothing tells the user it happened.

This document is for whoever touches `core/crates/inkuna-core/src/formats/epub/`
next. The lessons are the point; the tables are the reference.

## The eight rounds

| # | Door | Measured | Fix | Commit |
|---|------|----------|-----|--------|
| 1 | Per-entry reads were unbounded; the archive's declared `uncompressed_size` is attacker-controlled | 261 KB on disk → 268 MB inflated (1028x) | `MAX_XML_ENTRY_BYTES` (64 MiB), `MAX_COVER_BYTES` (16 MiB), enforced on the read via `take(cap + 1)`, never on the declared size | `31cf51e` |
| 2 | `Format::detect` did an uncapped `read_to_string` on the zip's `mimetype` entry — on *every* import, before any capped reader existed | 521 KB → **564 MB RSS** | `MAX_MIMETYPE_BYTES` = 256 (the spec fixes that entry to the 20-byte literal `application/epub+zip`) | `7068123` |
| 3 | The `<spine>` may list the same under-cap entry any number of times; each was decompressed concurrently by rayon and all were retained | 61 KB → **2.35 GB RSS, and the import SUCCEEDED** (~38,000x, against the 1028x round 1 was written for) | Extraction deduplicated by distinct resource, an aggregate corpus budget, `MAX_SPINE_ITEMS`, and a manifest `HashMap` replacing a quadratic id scan; then spine documents split from the mandatory XML parts, because the rayon-concurrent transient was still `threads × 64 MiB` ≈ 384 MB on a 6-core phone | `59f8a2c`, `41d0d7a` |
| 4 | The caps bounded *bytes decompressed*; nothing bounded what the parsers *built* from them | Unbounded `Vec<TocEntry>` → one `chapters` row each: 386 KB → **480 MB database, 2,544,000 rows, `Ok`**. Unbounded `Vec<ManifestItem>`: 355 KB → **616 MB RSS, `Ok`** | Caps moved to the push sites: `MAX_TOC_ENTRIES`, `MAX_TOC_DEPTH`, `MAX_MANIFEST_ITEMS`, `MAX_AUTHORS`. `MAX_SPINE_ITEMS` already existed but was applied via `.take()` *after* `parse_opf` had materialized the full `Vec` — the cap sat downstream of the allocation it was meant to prevent | `80f261c`, `1444d1b`, `a9d1db2` |
| 5 | A clone inside a loop: in `parse_ncx` the navPoint label lived in `labels` for the whole navPoint while the `<content src>` arm took a fresh `to_string()` per element. Same round: `MAX_HREF_BYTES` was checked on the manifest href *as written*, while resolution prepends the OPF directory | 2,165 bytes → **630.9 MB database**; 2,259 bytes → **1,255.9 MB**; ceiling ~640 GB. Every cap saw in-bounds numbers: one small entry, 10,000 entries, depth 1. The href door gave each of 10,000 itemrefs its own ~64 KiB resolved copy from a crafted `container.xml` | Only the first `<content>` per navPoint is taken (NCX fixes the content model to exactly one); `MAX_TOC_TOTAL_BYTES` added as an aggregate over what each entry *retains*; `MAX_HREF_BYTES` re-checked on the **resolved** href | `de1352b` |
| 6 | — (not a bypass) | — | A per-publication persistence circuit breaker (`MAX_PERSISTED_ROWS`, `MAX_PERSISTED_BYTES`), charged incrementally before every insert in `commit_import`, added at the owner's request as a backstop for the *unknown remainder* of the class. Crossing either returns `CoreError::InvalidPublication` inside the transaction, so the existing rollback and `cleanup_files(true)` undo everything | `8c0ea79` |
| 7 | `<dc:title>` / `<dc:creator>` had no per-value cap — and unlike spine text (read once at import) the title is re-materialized by `list()` and `search_library` on *every* call, across the UniFFI boundary | 61,688 bytes → a **62.9 MB title**; **305 ms per keystroke** for one book, **1.47 s per keystroke and 630 MB RSS** for four (247 KB of files). Post-fix: 40–105 µs per keystroke | `MAX_METADATA_VALUE_BYTES` = 2048, enforced in the accumulator at the push site, cut on a `char` boundary for CJK safety | `bbf54f0` |
| 8 | The caps' own declared worst case was itself the harm | 18 KB → 181 MB database (364 MB with WAL); 144 KB → **1.45 GB** via `import_batch` | `MAX_TOTAL_TEXT_BYTES` 128 → 32 MiB; the spine deduplicated on the resolved href before rows are persisted; `MAX_PERSISTED_BYTES` re-derived 384 → 128 MiB | `9bf850e` |

All measurements were taken on release builds driven through the public
`Library::import` API under `/usr/bin/time -l`.

A ninth commit, `5591e74`, is not a door but is worth knowing about: the
aggregate text budget's first implementation charged retention from inside
the rayon workers, so *which* resources survived depended on scheduling,
and a rejected text was never credited back — one over-budget resource
poisoned the pre-read check and dropped every later resource however small.
Hardening code introduces its own bugs; the budget is now a work guard plus
a sequential decision pass.

## Lessons

### The recurring shape: a value cloned inside an attacker-controlled loop

Five of the eight doors had exactly this form — the repeated spine entry,
the repeated `resource_text` copy, the manifest href cloned per spine
reference, the NCX label cloned per `<content>`, the resolved href copied
per itemref. It is the first thing to grep for when auditing a parser: find
every `clone()` / `to_string()` / `to_owned()` / `push(...)` whose loop
count comes from the file, and multiply the two bounds together by hand.

### Bound the parse product, not just the input bytes

A byte cap bounds what you read; it does not bound what you build. Measured
here, the parsed structure ran roughly 10x the decompressed bytes — and in
the TOC case it did not stay in memory, it became permanent database rows.
`MAX_XML_ENTRY_BYTES` at 64 MiB is not an answer to "how much can a 64 MiB
OPF cost"; `MAX_MANIFEST_ITEMS` is.

### Enforce at the push site, inside the loop

A cap applied after a collection is materialized bounds the *stored* value
while the allocation already happened. `.take()` on a finished `Vec` is not
a cap. Every bound in this parser now sits at the `push` — including the
metadata accumulator, which never grows past its cap rather than being
truncated afterwards.

### Ask where the value is *spent*, not only how big it is

The metadata door was the most damaging per byte of any of the eight, and
by size it looked like the mildest. The title sits on the hottest read path
in the app: `list()` selects it on every launch and `search_library` folds
it per publication per keystroke, and each result crosses the FFI boundary
as an owned `String`. A byte admitted there is paid forever, not once at
import.

### `Ok` with damage is far worse than an error

The two worst findings both returned success while writing hundreds of MB
into the user's personal library. Severity in this class is not "how much
memory" — it is "does the user find out". This is also why the parser's
degrade-vs-fail split is deliberate rather than uniform: an optional part
(TOC, cover, one chapter's text) degrades with a log line, a mandatory part
(container, OPF, manifest) fails cleanly, and the persistence breaker
converts a silent success into a rollback.

### A cap with no test is a cap that isn't load-bearing yet

QC's own observation across three rounds: every cap that had a test
survived probing, and all three holes it found were in caps without one.
Two tests in this history also passed *for the wrong reason* — one asserted
zero rows in a fixture that could never have produced rows anyway. The
check that matters is whether the test goes **red** when you neutralize the
cap, not whether it is green today.

### Per-part caps cannot be proven complete; a budget can be a backstop

Hence the circuit breaker in `features/import/budget.rs`. Be precise about
what it does:

- It bounds **persistence** — rows and variable-length bytes written by one
  `commit_import` — charged incrementally *before* each insert, so a bomb
  aborts within one row of the ceiling rather than after gigabytes of
  transient WAL have already grown on disk.
- It does **not** bound peak RSS. Parse products are built before any row
  is written, so a parse-time bomb that never reaches the DB (the 616 MB
  manifest spike, for one) is out of its reach entirely. The parse-time
  caps remain the only defense for those, and are still load-bearing.

### Sizing a cap is a two-sided risk

Too loose and it admits the attack; too tight and it silently mangles a
real book, which is its own kind of data loss. `MAX_TOC_ENTRIES` was raised
10,000 → 100,000 in `de1352b` precisely because a verse-level Bible nav
would have been silently truncated. The calibration references actually
used, worth reusing:

- A very large real novel's full text is a few MB; omnibus "complete works"
  editions stay under ~20 MB.
- A real title or creator name is under ~200 bytes even in CJK.
- The most extreme honest TOC known — a verse-level Bible — is ~31,000
  entries and retains under 2 MB.
- Real hrefs are archive paths a few dozen bytes long.

### Independent adversarial review is what found every one of these

No round found its own successor's bug. Rounds 1–5 and 7–8 were each closed
by someone who had not written the previous fix. Budget review capacity for
this kind of code accordingly.

## Current bounds (read from HEAD)

Paths are relative to `core/crates/inkuna-core/src/`.

| Constant | Value | File | Bounds | Exceeding it |
|----------|-------|------|--------|--------------|
| `MAX_MIMETYPE_BYTES` | 256 B | `formats/format.rs` | bytes read from the zip's `mimetype` entry during format detection | degrades — the file is simply not detected as EPUB (falls through to CBZ) |
| `MAX_XML_ENTRY_BYTES` | 64 MiB | `formats/epub/archive.rs` | decompressed bytes of one mandatory XML part: `container.xml`, the OPF, the nav doc, the NCX | fails (`InvalidPublication`) for container/OPF; degrades to no TOC for nav/NCX, whose callers use `if let Ok` |
| `MAX_SPINE_ENTRY_BYTES` | 8 MiB | `formats/epub/archive.rs` | one spine content document, read concurrently | degrades — that resource loses its text row, logged |
| `MAX_COVER_BYTES` | 16 MiB | `formats/epub/archive.rs` | cover image bytes | degrades — import continues with no cover |
| `MAX_MANIFEST_ITEMS` | 100,000 | `formats/epub/opf.rs` | `<item>` entries retained | fails (`InvalidPublication`) — the manifest is a mandatory part |
| `MAX_SPINE_ITEMS` | 10,000 | `formats/epub/opf.rs` | `<itemref>` idrefs retained | degrades — the rest are never materialized; `spine_itemrefs_seen` lets the caller warn |
| `MAX_HREF_BYTES` | 4096 B | `formats/epub/opf.rs` | a manifest item's href as written, and again the **resolved** spine href in `formats/epub/package.rs` | degrades — the item / itemref is skipped like an unresolvable idref |
| `MAX_AUTHORS` | 1,000 | `formats/epub/opf.rs` | retained `<dc:creator>` values | degrades — extras dropped, `creators_seen` warns |
| `MAX_METADATA_VALUE_BYTES` | 2048 B | `formats/epub/opf.rs` | each retained `dc:` value (`title`, each `creator`, `language`), enforced in the accumulator | degrades — cut on a `char` boundary, `truncated_metadata_values` warns |
| `MAX_TOC_ENTRIES` | 100,000 | `formats/epub/toc.rs` | TOC entries retained (nav and NCX) | degrades — the parse stops, warned |
| `MAX_TOC_TOTAL_BYTES` | 8 MiB | `formats/epub/toc.rs` | aggregate of each retained entry's `title.len() + href.len()` | degrades — the parse stops, warned |
| `MAX_TOC_DEPTH` | 64 | `formats/epub/toc.rs` | `<ol>` nesting (nav) / open `<navPoint>`s (NCX); the NCX overflow is counted, never allocated | degrades — deeper entries skipped, warned once |
| `MAX_TOTAL_TEXT_BYTES` | 32 MiB | `formats/epub/text.rs` | the aggregate retained corpus, charged per **retained copy** in spine order | degrades — remaining resources lose their text rows, warned once |
| `MAX_EXTENSION_LEN` | 8 | `formats/epub/cover.rs` | href suffix length accepted as a cover file extension | degrades — no cover |
| `MAX_PERSISTED_ROWS` | 150,000 | `features/import/budget.rs` | rows one publication may insert across `publications`, `chapters`, `resources`, `resource_text` | fails (`InvalidPublication`) — transaction rolls back, staged book and cover are swept |
| `MAX_PERSISTED_BYTES` | 128 MiB | `features/import/budget.rs` | variable-length bytes one publication may persist | fails, same path |

Structural bounds with no constant, equally load-bearing:

- `formats/epub/package.rs` deduplicates the spine on the **resolved** href
  (EPUB 3 requires unique itemrefs), so a repeating spine cannot persist a
  `resources` row and a text copy per repeat.
- `formats/epub/text.rs` extracts each **distinct** resource once and
  aliases the `Arc<str>` across repeats.
- `formats/epub/package.rs` pairs idrefs to manifest items through a
  `HashMap`, not a linear `find` — the scan made the pairing quadratic, a
  CPU hang out of a ~100 KB file.
- All three parsers set `reader.config_mut().check_end_names = false`;
  quick-xml's open-element stack is otherwise a growth vector on crafted
  nesting.
- `read_entry_capped` takes `cap + 1` bytes and checks the cap **before**
  decoding UTF-8, so a CJK entry cut mid-character surfaces as the cap
  rejection it is rather than as invalid UTF-8.

## Checklist for anyone touching this parser

Concrete, greppable, in the order that has actually caught things:

1. **Does this new `String`/`Vec` get cloned or pushed once per iteration of
   something the file controls?** Name the two bounds — per-item size and
   iteration count — and multiply them. If the product is not a number you
   would accept on a phone, you need a third bound over the product.
2. **Is the cap at the push site?** `if v.len() < CAP { v.push(x) }` inside
   the loop, not `v.into_iter().take(CAP)` after the loop.
3. **Does the cap bound the parse product, or only the input bytes?** If the
   only cap is a byte cap on the entry, the product is unbounded.
4. **Does anything downstream retain a per-repeat copy?** Deduplicate on the
   value that is actually persisted (the *resolved* href, not the one as
   written) before rows are written.
5. **Is the value re-materialized on a read path?** Anything reachable from
   `list()`, `search_library`, or the FFI boundary is paid per call and per
   keystroke, forever. Cap those far tighter than import-only values.
6. **Is a cap checked before *and* after a transformation that can grow the
   value?** `MAX_HREF_BYTES` is checked twice for exactly this reason.
7. **Degrade or fail?** Optional part → degrade with a `log::warn!` naming
   the archive path and the cap; mandatory part → `CoreError::InvalidPublication`.
   Never degrade silently.
8. **Does the test go red if you neutralize the cap?** Temporarily raise the
   constant and re-run. A test that stays green is not testing the cap. Also
   check the fixture could have produced the thing you assert is absent.
9. **Is the honest side pinned too?** Every tightening needs a companion test
   that a real book (including a CJK one) is unaffected — see
   `honest_cjk_book_imports_unaffected_by_the_budget` in
   `features/import/tests.rs`.
10. **Did you charge the persistence budget?** Any new insert in
    `commit_import` must call `budget.charge(...)` before its `tx.execute`,
    or the breaker no longer bounds the commit.

## What remains

Accepted residuals, verified against HEAD:

- **Transient peak is still roughly `rayon threads × MAX_SPINE_ENTRY_BYTES`.**
  Text extraction reads distinct resources in parallel, so a 6-core device
  can hold ~48 MB of in-flight chapter text on top of the retained corpus.
  The aggregate budget cannot prevent this: reads are already in flight when
  it trips. 8 MiB was chosen to make that product tolerable, not to remove it.
- **`import_batch` multiplies the per-publication budget by concurrency.**
  It is a `par_iter` over paths and each `import` constructs its own
  `PersistBudget::for_import()`; nothing bounds the batch as a whole or the
  number of paths. N crafted files can persist N × 128 MiB. This is why
  round 8 re-derived the per-publication ceiling downwards rather than
  relying on it alone.
- **Single `1x`-bounded values still cost their cap once.** A crafted OPF
  costs its 64 MiB read even though nothing is amplified from it. Worse,
  `read_package` holds `container` and `opf_xml` live for the whole
  function while a nav/NCX read is in flight, so a crafted publication can
  hold ~192 MiB of mandatory XML strings at once — inside jetsam range on
  its own. Nothing has closed this; if the OPF cap is ever revisited,
  dropping `container` after `rootfile_path` and streaming the OPF would
  cut it by two thirds.
- **The breaker does not bound memory.** Restated because it is easy to
  misremember as general protection: it bounds only what `commit_import`
  writes. A parse-time bomb that OOMs before the transaction opens is
  unaffected by it.
