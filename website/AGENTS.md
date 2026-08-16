# Inkuna Website Rules

You are working inside `website/`, the marketing site for
[inkuna.app](https://inkuna.app), built with Astro and deployed via
Cloudflare Pages.

## Tech Stack & Deployment

| Layer | Technology | Notes |
|------|------|------|
| Site | Astro (static output, zero client JS) | `src/pages/*.astro` + shared `src/layouts/Base.astro`; latest Astro per the workspace "latest everything" policy |
| Hosting | Cloudflare Pages (direct upload) | project `inkuna`; deployed by `.github/workflows/deploy-website.yml`: `pnpm install --frozen-lockfile && pnpm run build`, then `wrangler pages deploy dist` |
| Packages | pnpm (only supported package manager) | version pinned by `packageManager` in `package.json`; `pnpm-lock.yaml` is committed, settings live in `pnpm-workspace.yaml` |
| Deploy CLI | wrangler, a pinned devDependency | so `wrangler-action` reuses the lockfile version instead of installing one at deploy time |
| Headers | `public/_headers` | Cloudflare Pages header rules (security headers), copied verbatim into `dist/` |

Every push to `main` that touches `website/` (or the deploy workflow) deploys
automatically. Local commands (run inside `website/`): `pnpm install` once,
then `pnpm dev` for a dev server and `pnpm build` for the production `dist/`.
Manual deploy from this machine: `pnpm build && pnpm exec wrangler pages deploy
dist --project-name inkuna --branch main`.

Never use `npm`/`npx`/`yarn` here: they would resolve outside `pnpm-lock.yaml`
and leave a stray `package-lock.json` (both foreign lockfiles are gitignored as
a backstop). Adding a dependency is `pnpm add <pkg>`; bumping is `pnpm update
--latest` (the workspace "latest everything" policy applies to pnpm, Astro and
wrangler themselves too — bump the `packageManager` pin when pnpm releases a
new stable). Two pnpm behaviours to know about:

- **Install scripts are blocked by default.** If a new dependency genuinely
  needs one, allow exactly it in `pnpm-workspace.yaml` under `allowBuilds` —
  today that is `esbuild` (Astro's bundler) and `workerd` (wrangler's runtime),
  both of which postinstall a platform binary. A missing entry fails the
  install with `ERR_PNPM_IGNORED_BUILDS`, in CI too.
- **Freshly published versions are rejected.** pnpm verifies the lockfile
  against a minimum-release-age policy, so a dependency published in the last
  couple of days fails resolution (`ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`)
  until it ages out. Wait it out rather than relaxing the policy.

`node_modules/`, `dist/`, and `.astro/` are gitignored — never commit build
output. `pnpm-lock.yaml` is not: commit it with every dependency change.

## Structure

- `src/layouts/Base.astro` — the one shared shell: theme tokens, reset, font
  stack, favicon, link styles, and head metadata (title/description/OG/robots
  and hreflang alternates via props). Every page uses it.
- `src/pages/` — one `.astro` file per route; page-specific CSS lives in the
  page's (or its component's) scoped `<style>` block.
- `src/components/` — markup shared across routes (e.g. `Home.astro`, the
  homepage rendered by all three locale routes).
- `src/i18n/` — `locales.ts` (locale list), `utils.ts` (`useTranslations`),
  and one dictionary module per page or shared component (`home.ts`,
  `footer.ts`, `support.ts`, `privacy.ts`, …).
- `src/lib/` — build-time helpers (e.g. `androidBeta.ts`, which resolves the
  latest `android-v*` release's APK URL from the GitHub API during the build;
  the deploy workflow re-runs on every release publish so the baked-in link
  stays current, and falls back to the releases page when the API is
  unreachable).
- `public/` — files copied verbatim to the site root (`_headers`).
- Planned growth: a changelog/blog section (use Astro content collections) —
  structure new work so it slots in cleanly.

## i18n / l10n

Astro's built-in i18n routing (no third-party plugin): locales `en`
(`/en/`), `ja` (`/ja/`), `zh` (`/zh/`) — every locale, English included, is
prefixed via `routing.prefixDefaultLocale`; `/` is a static 301 to `/en/`
(the old unprefixed English home), defined in the `redirects` block of
`astro.config.mjs`. Conventions:

- All user-facing strings live in `src/i18n/` dictionaries — never hard-code
  copy in a shared component. One dictionary module per page or shared
  component (`home.ts`, `footer.ts`, `support.ts`, `privacy.ts`, …), each
  exporting a `Dictionary<T>` consumed via `useTranslations(dict, lang)`;
  never one grab-bag file for the whole site. Strings wrapping a link are
  split into `.pre`/`.post` keys so each locale keeps its own word order.
- Every content page should exist in all three locales: render one shared
  component per route, pass `localizedPath` to `Base` (emits
  hreflang/x-default alternates), and include the footer language switcher.
  `404.astro` is the deliberate exception (single page, English).
- Build locale URLs with `getRelativeLocaleUrl`/`getAbsoluteLocaleUrl` from
  `astro:i18n`, never by string concatenation.
- CJK has no true italics — pages must neutralize italic styles for CJK
  (`:lang(ja)`, `:lang(zh)`), as `Home.astro` does for the tagline.
- Adding a locale: extend `languages` in `src/i18n/locales.ts`, add the
  locale's entry to every dictionary module in `src/i18n/`, add the locale to
  `astro.config.mjs`, and add the one-line page stubs under
  `src/pages/<locale>/`.
- Adding a page: create its dictionary module in `src/i18n/` (all three
  locales), a shared component in `src/components/`, and one-line stubs in
  every `src/pages/<locale>/` directory; link it from the footer if it is a
  permanent page.

## Conventions

- **Zero client-side requests beyond the page itself**: no CDN scripts, web
  fonts, analytics, remote images, or shipped JavaScript. Everything is
  inlined at build time or in-repo; favicons are static files in `public/`,
  regenerated from `assets/brand/` via `scripts/gen-icons.sh`. This keeps the
  site fast, private, and dependency-light. Astro's static build satisfies
  this by default — don't add integrations or client islands that break it.
- **No UI or CSS framework**: no React (nothing here is interactive) and no
  Tailwind — the hand-tuned scoped CSS on shared tokens is the design. If a
  genuinely interactive island ever becomes necessary, add a framework
  integration for that island only and keep the rest zero-JS.
- **Ink & moonlight theming**: light ("ink on paper") and dark ("moonlight")
  palettes via `prefers-color-scheme`, defined once as tokens in
  `Base.astro`, mirroring the app's planned reader themes. Any new page must
  support both.
- **Literary serif stack** with CJK fallbacks (`Songti SC`,
  `Hiragino Mincho ProN`, `Yu Mincho`) — CJK readers are a primary audience.
- **No personal information** (names, emails) anywhere on the site; link to
  the GitHub organization/repo, not individuals. Deliberate exception: the
  support contact address `dev@zheng-she.com` on the support and privacy
  pages — App Store Connect requires a reachable support contact.
- Keep copy quiet and literary; no marketing superlatives, no screenshots of
  unfinished UI.
- Set `lang` correctly: pages via the layout's `lang` prop, inline CJK text
  via `lang="ja"`/`lang="zh"` spans so fonts resolve properly.
