import type { Dictionary } from "./utils";

// Strings that wrap a link are split into .pre / .post around the link text
// so each locale keeps its own word order.
const en = {
  "meta.title": "Inkuna — a minimalist book reader",
  "meta.description":
    "Inkuna is a minimalist book reader where ink meets moonlight. EPUB and more, first-class CJK typography, native on iOS and Android. Open source under AGPL-3.0.",
  "og.description": "A minimalist book reader where ink meets moonlight.",
  tagline: "A minimalist book reader where ink meets moonlight.",
  intro:
    "Crafted, quiet, literary. Inkuna keeps your library close and stays out of the way of the page — no accounts, no noise, just reading.",
  "features.formats":
    "EPUB, MOBI, AZW3 (DRM-free), TXT, and PDF today; CBZ/CBR comics planned.",
  "features.cjk":
    "First-class CJK typography — vertical writing and CJK-aware search are core goals, not afterthoughts.",
  "features.native": "Fully native on iOS and Android, sharing one Rust core.",
  "features.license.pre": "Free and open source under ",
  "features.license.post": ".",
  "platforms.pre":
    "Inkuna is in active development. Follow along or build it yourself on ",
  "platforms.post":
    " — App Store and Play Store releases will land when the reading experience earns them.",
};

const ja: typeof en = {
  "meta.title": "Inkuna — ミニマルな読書アプリ",
  "meta.description":
    "Inkunaは、墨と月光が出会うミニマルな読書アプリ。EPUBをはじめ多くの形式に対応し、縦書きなどCJKタイポグラフィを第一級に扱います。iOS・Androidネイティブ、AGPL-3.0のオープンソース。",
  "og.description": "墨と月光が出会う、ミニマルな読書アプリ。",
  tagline: "墨と月光が出会う、ミニマルな読書アプリ。",
  intro:
    "静かに、丁寧に、文学的に。Inkunaは蔵書をそばに置きながら、ページの邪魔をしません — アカウントもノイズもなく、ただ読むことだけを。",
  "features.formats":
    "EPUB・MOBI・AZW3（DRMフリー）・TXT・PDFに対応。CBZ/CBRコミックも計画中。",
  "features.cjk":
    "縦書きやCJK対応検索を後回しにしない、第一級のCJKタイポグラフィ。",
  "features.native": "iOSとAndroidで完全ネイティブ。ひとつのRustコアを共有。",
  "features.license.pre": "",
  "features.license.post": "のもとで自由に使えるオープンソース。",
  "platforms.pre": "Inkunaは現在開発中です。",
  "platforms.post":
    "で開発を追うことも、自分でビルドすることもできます — App Store / Google Playでの公開は、読書体験がそれに値するようになってから。",
};

const zh: typeof en = {
  "meta.title": "Inkuna — 极简阅读应用",
  "meta.description":
    "Inkuna 是一款墨与月光相遇的极简阅读应用。支持 EPUB 等多种格式，将竖排与 CJK 排版视为一等公民。iOS 与 Android 全原生，以 AGPL-3.0 开源。",
  "og.description": "墨与月光相遇的极简阅读应用。",
  tagline: "墨与月光相遇的极简阅读应用。",
  intro:
    "克制、安静、有书卷气。Inkuna 让书库常伴左右，却从不打扰页面本身 —— 没有账号，没有噪音，只有阅读。",
  "features.formats":
    "现已支持 EPUB、MOBI、AZW3（无 DRM）、TXT 与 PDF；CBZ/CBR 漫画在计划中。",
  "features.cjk":
    "一流的 CJK 排版 —— 竖排与 CJK 感知搜索是核心目标，而非事后补充。",
  "features.native": "iOS 与 Android 完全原生，共享同一个 Rust 内核。",
  "features.license.pre": "以 ",
  "features.license.post": " 自由开源。",
  "platforms.pre": "Inkuna 正在积极开发中。欢迎在 ",
  "platforms.post":
    " 上关注或自行构建 —— App Store 与 Play 商店版本将在阅读体验足够成熟时发布。",
};

export const home: Dictionary<typeof en> = { en, ja, zh };
