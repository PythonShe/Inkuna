import type { Dictionary } from "./utils";

const en = {
  "meta.title": "Changelog — Inkuna",
  "meta.description": "What changed in each Inkuna release for Android.",
  h1: "Changelog",
  intro:
    "What changed in each Android release, newest first. On iOS, release notes appear in TestFlight and the App Store.",
  "empty.pre": "The release history could not be loaded here — find it on ",
  "empty.link": "GitHub releases",
  "empty.post": ".",
  /** BCP 47 tag for build-time date formatting. */
  dateLocale: "en",
};

const ja: typeof en = {
  "meta.title": "更新履歴 — Inkuna",
  "meta.description": "Android 版 Inkuna の各リリースの変更内容。",
  h1: "更新履歴",
  intro:
    "Android 版の各リリースの変更内容を、新しい順に掲載しています。iOS 版のリリースノートは TestFlight と App Store でご覧いただけます。",
  "empty.pre": "リリース履歴を読み込めませんでした。",
  "empty.link": "GitHub リリース",
  "empty.post": "をご覧ください。",
  dateLocale: "ja",
};

const zh: typeof en = {
  "meta.title": "更新日志 — Inkuna",
  "meta.description": "Inkuna Android 版各版本的更新内容。",
  h1: "更新日志",
  intro:
    "Android 版各版本的更新内容，按时间倒序排列。iOS 版的更新说明请在 TestFlight 或 App Store 中查看。",
  "empty.pre": "暂时无法加载发布记录，请前往 ",
  "empty.link": "GitHub 发布页",
  "empty.post": "查看。",
  dateLocale: "zh-CN",
};

export const changelog: Dictionary<typeof en> = { en, ja, zh };
