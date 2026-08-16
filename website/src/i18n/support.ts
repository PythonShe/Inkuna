import type { Dictionary } from "./utils";

const en = {
  "meta.title": "Support — Inkuna",
  "meta.description":
    "Get help with Inkuna. Report a bug, request a feature, or find answers to common questions.",
  h1: "Support",
  intro:
    "Inkuna is in active development, and we'd love to hear from you. The fastest way to get help is through the GitHub repository.",
  "bug.h": "Report a bug",
  "bug.p":
    "Found something broken? Open an issue on GitHub. Please include your device, OS version, app version, and a short description of what happened.",
  "bug.link": "Report a bug",
  "feature.h": "Request a feature",
  "feature.p":
    "Have an idea that would make Inkuna better? Every suggestion is read and appreciated.",
  "feature.link": "Share an idea",
  "faq.h": "Common questions",
  faq: [
    {
      q: "Is Inkuna free?",
      a: "Yes. Inkuna is free and open source under the AGPL-3.0 license.",
    },
    {
      q: "Which formats are supported?",
      a: "EPUB, MOBI, AZW3 (DRM-free), TXT, and PDF today; CBZ/CBR comics are planned.",
    },
    {
      q: "Do I need an account?",
      a: "No. Inkuna has no accounts, no sign-up, and no tracking.",
    },
    {
      q: "When will Inkuna be on the App Store?",
      a: "Inkuna is in active development. Releases will land when the reading experience earns them.",
    },
  ],
};

const ja: typeof en = {
  "meta.title": "サポート — Inkuna",
  "meta.description":
    "Inkuna に関するヘルプ。バグの報告、機能のリクエスト、よくある質問への回答。",
  h1: "サポート",
  intro:
    "Inkuna は現在も開発を続けています。ご意見・ご要望をお待ちしています。最も早く解決できるのは GitHub リポジトリからです。",
  "bug.h": "バグを報告する",
  "bug.p":
    "不具合を見つけましたか？ GitHub で issue を開いてください。機種・OS バージョン・アプリバージョンに加え、何が起きたのかを簡単に添えていただけると助かります。",
  "bug.link": "バグを報告する",
  "feature.h": "機能をリクエストする",
  "feature.p":
    "Inkuna をより良くするアイデアはありますか？ すべてのご意見に目を通しています。",
  "feature.link": "アイデアを共有する",
  "faq.h": "よくある質問",
  faq: [
    {
      q: "Inkuna は無料ですか？",
      a: "はい。AGPL-3.0 ライセンスのオープンソースとして無料で公開しています。",
    },
    {
      q: "対応している形式は？",
      a: "EPUB・MOBI・AZW3（DRM フリー）・TXT・PDF に対応。CBZ/CBR コミックは計画中です。",
    },
    {
      q: "アカウントは必要ですか？",
      a: "いいえ。Inkuna にアカウントや登録、トラッキングはありません。",
    },
    {
      q: "App Store での公開はいつですか？",
      a: "Inkuna は現在開発中です。読書体験がそれに値するようになってから公開します。",
    },
  ],
};

const zh: typeof en = {
  "meta.title": "帮助与支持 — Inkuna",
  "meta.description": "获取 Inkuna 帮助。报告问题、提出建议，或查看常见问题。",
  h1: "帮助与支持",
  intro:
    "Inkuna 仍在积极开发中，欢迎与我们交流。最快捷的途径是通过 GitHub 仓库。",
  "bug.h": "报告问题",
  "bug.p":
    "发现了问题？请在 GitHub 上提交 Issue，并附上设备型号、系统与 App 版本，以及简要的问题描述。",
  "bug.link": "报告问题",
  "feature.h": "提出建议",
  "feature.p": "有让 Inkuna 更好的想法？每一条建议我们都会认真阅读。",
  "feature.link": "分享想法",
  "faq.h": "常见问题",
  faq: [
    {
      q: "Inkuna 免费吗？",
      a: "是的。Inkuna 以 AGPL-3.0 协议开源，完全免费。",
    },
    {
      q: "支持哪些格式？",
      a: "现已支持 EPUB、MOBI、AZW3（无 DRM）、TXT 与 PDF；CBZ/CBR 漫画在计划中。",
    },
    {
      q: "需要账号吗？",
      a: "不需要。Inkuna 没有账号、注册，也没有追踪。",
    },
    {
      q: "何时在 App Store 上架？",
      a: "Inkuna 正在积极开发中，当阅读体验足够成熟时自会上架。",
    },
  ],
};

export const support: Dictionary<typeof en> = { en, ja, zh };
