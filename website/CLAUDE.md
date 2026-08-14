# Inkuna Website Rules

You are working inside `website/`, the static marketing site for
[inkuna.app](https://inkuna.app), deployed via Cloudflare Pages.

## Tech Stack & Deployment

| Layer | Technology | Notes |
|------|------|------|
| Site | Hand-written static HTML/CSS | no framework, no build step — minimalism is the point |
| Hosting | Cloudflare Pages (git integration) | root directory `website`, build command empty, output directory `.` |
| Headers | `_headers` | Cloudflare Pages header rules (security headers) |

Every push to `main` that touches `website/` deploys automatically once the
Pages project is connected to this repository.

## Conventions

- **Zero external requests**: no CDN scripts, web fonts, analytics, or remote
  images. Everything is inline or in-repo; favicons are data URIs. This keeps
  the site fast, private, and dependency-free.
- **Ink & moonlight theming**: light ("ink on paper") and dark ("moonlight")
  palettes via `prefers-color-scheme`, mirroring the app's planned reader
  themes. Any new page must support both.
- **Literary serif stack** with CJK fallbacks (`Songti SC`,
  `Hiragino Mincho ProN`, `Yu Mincho`) — CJK readers are a primary audience.
- **No personal information** (names, emails) anywhere on the site; link to
  the GitHub organization/repo, not individuals.
- Keep copy quiet and literary; no marketing superlatives, no screenshots of
  unfinished UI.
- New pages get the same self-contained treatment: own inline `<style>`,
  both themes, `lang` attribute set correctly (use `lang="ja"`/`lang="zh"`
  spans for CJK text so fonts resolve properly).
