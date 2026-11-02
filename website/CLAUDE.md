# Inkuna Website

Static Astro site for `inkuna.app` on Cloudflare Pages (project `inkuna`).

## Build and deploy

- Run inside `website/`: `pnpm dev`, `pnpm build` (outputs `dist/`).
- `.github/workflows/deploy-website.yml` deploys on every `main` push touching
  `website/` and on each release publish. Manual deploy:
  `pnpm build && pnpm exec wrangler pages deploy dist --project-name inkuna --branch main`.
- wrangler is a pinned devDependency so `wrangler-action` reuses the lockfile
  version.
- Dependency install scripts are blocked; allow one only by adding it under
  `allowBuilds` in `pnpm-workspace.yaml` (today `esbuild` and `workerd`). A
  missing entry fails install with `ERR_PNPM_IGNORED_BUILDS`, CI included.
- pnpm rejects versions published in the last couple of days
  (`ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`). Wait it out; don't relax the
  policy.
- Bump the `packageManager` pin in `package.json` when pnpm releases a new
  stable.

## Structure

- `src/layouts/Base.astro`: the one shared shell (theme tokens, reset, font
  stack, favicon, head metadata, hreflang alternates). Every page uses it.
- `src/pages/<locale>/`: one-line route stubs rendering a shared component
  from `src/components/`; page CSS lives in the component's scoped `<style>`.
- `src/i18n/`: `locales.ts`, `utils.ts` (`useTranslations`), and one
  dictionary module per page or component.
- `src/lib/`: build-time GitHub API lookups (`androidBeta.ts` for the latest
  `android-v*` APK link, `changelog.ts` for release notes), each falling back
  gracefully when the API is unreachable.
- `public/`: copied verbatim (`_headers` security headers, favicons
  regenerated from `assets/brand/` by `scripts/gen-icons.sh`).

## i18n

- Locales `en`, `ja`, `zh`, all prefixed (`prefixDefaultLocale`); `/` is a
  static 301 to `/en/` in `astro.config.mjs`.
- All user-facing copy lives in `src/i18n/` dictionaries (`Dictionary<T>` via
  `useTranslations(dict, lang)`), never in components and never one grab-bag
  file. Split strings around a link into `.pre` / `.post` keys.
- Every content page exists in all three locales, passes `localizedPath` to
  `Base`, and includes the footer language switcher. `404.astro` is the
  English-only exception.
- Build locale URLs with `getRelativeLocaleUrl` / `getAbsoluteLocaleUrl` from
  `astro:i18n`, never by concatenation.
- New locale: extend `src/i18n/locales.ts`, every dictionary module,
  `astro.config.mjs`, and add `src/pages/<locale>/` stubs. New page: a
  dictionary module (all locales), a component, stubs in every locale
  directory, and a footer link if permanent.

## Conventions

- Zero client-side requests beyond the page: no CDN scripts, web fonts,
  analytics, remote images, or shipped JavaScript. No UI or CSS framework
  (no React, no Tailwind); if an interactive island is ever needed, add a
  framework for that island only.
- Light ("ink on paper") and dark ("moonlight") palettes via
  `prefers-color-scheme`, defined once as tokens in `Base.astro`; every page
  supports both.
- Serif stack with CJK fallbacks (`Songti SC`, `Hiragino Mincho ProN`,
  `Yu Mincho`). CJK has no true italics: neutralize italics under
  `:lang(ja)` / `:lang(zh)`. Set `lang` on pages (layout prop) and on inline
  CJK spans.
- The only personal contact allowed is `dev@zheng-she.com` (support, privacy,
  and home pages), because App Store Connect requires a reachable support
  address. Link to the GitHub repo, not individuals.
- Copy is quiet and literary: no superlatives, no screenshots of unfinished
  UI.
