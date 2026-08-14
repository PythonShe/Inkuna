# Inkuna Website Rules

You are working inside `website/`, the marketing site for
[inkuna.app](https://inkuna.app), built with Astro and deployed via
Cloudflare Pages.

## Tech Stack & Deployment

| Layer | Technology | Notes |
|------|------|------|
| Site | Astro (static output, zero client JS) | `src/pages/*.astro` + shared `src/layouts/Base.astro`; latest Astro per the workspace "latest everything" policy |
| Hosting | Cloudflare Pages (direct upload) | project `inkuna`; deployed by `.github/workflows/deploy-website.yml`: `npm ci && npm run build`, then `wrangler pages deploy dist` |
| Headers | `public/_headers` | Cloudflare Pages header rules (security headers), copied verbatim into `dist/` |

Every push to `main` that touches `website/` (or the deploy workflow) deploys
automatically. Local commands (run inside `website/`): `npm run dev` for a dev
server, `npm run build` for the production `dist/`. Manual deploy from this
machine: `npm run build && npx wrangler pages deploy dist --project-name
inkuna --branch main`.

`node_modules/`, `dist/`, and `.astro/` are gitignored — never commit build
output.

## Structure

- `src/layouts/Base.astro` — the one shared shell: theme tokens, reset, font
  stack, favicon, link styles, and head metadata (title/description/OG/robots
  via props). Every page uses it.
- `src/pages/` — one `.astro` file per route; page-specific CSS lives in the
  page's own scoped `<style>` block.
- `public/` — files copied verbatim to the site root (`_headers`).
- Planned growth: a changelog/blog section (use Astro content collections)
  and localized pages (use Astro's built-in i18n routing) — structure new
  work so those slot in cleanly.

## Conventions

- **Zero client-side requests beyond the page itself**: no CDN scripts, web
  fonts, analytics, remote images, or shipped JavaScript. Everything is
  inlined at build time or in-repo; favicons are data URIs. This keeps the
  site fast, private, and dependency-light. Astro's static build satisfies
  this by default — don't add integrations or client islands that break it.
- **Ink & moonlight theming**: light ("ink on paper") and dark ("moonlight")
  palettes via `prefers-color-scheme`, defined once as tokens in
  `Base.astro`, mirroring the app's planned reader themes. Any new page must
  support both.
- **Literary serif stack** with CJK fallbacks (`Songti SC`,
  `Hiragino Mincho ProN`, `Yu Mincho`) — CJK readers are a primary audience.
- **No personal information** (names, emails) anywhere on the site; link to
  the GitHub organization/repo, not individuals.
- Keep copy quiet and literary; no marketing superlatives, no screenshots of
  unfinished UI.
- Set `lang` correctly: pages via the layout's `lang` prop, inline CJK text
  via `lang="ja"`/`lang="zh"` spans so fonts resolve properly.
