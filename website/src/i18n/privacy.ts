import type { Dictionary } from "./utils";

const en = {
  "meta.title": "Privacy Policy — Inkuna",
  "meta.description":
    "Inkuna's privacy policy. The app collects no personal information — no accounts, no analytics, no tracking.",
  h1: "Privacy Policy",
  updated: "Last updated August 17, 2026",
  intro:
    "Inkuna is designed to be quiet and private. Reading is personal, and your library should stay yours.",
  "summary.h": "In short",
  "summary.p":
    "Inkuna collects no personal information. There are no accounts, no analytics, no ads, and no tracking. Everything you read stays on your device.",
  "collect.h": "What we collect",
  "collect.p":
    "Nothing. Inkuna is a fully offline reading app. Your library, reading progress, and settings are stored only on your device.",
  "practices.h": "No accounts, no ads, no tracking",
  "practices.p":
    "No account is required to use Inkuna. We never sell or share data, show ads, or use third-party analytics or tracking SDKs.",
  "reading.h": "Your reading data",
  "reading.p":
    "Your library and reading positions never leave your device. They are not uploaded, synced, or otherwise accessible to us.",
  "website.h": "The website",
  "website.p":
    "The Inkuna website is a static page. It sets no cookies, runs no analytics, and loads no third-party scripts.",
  "changes.h": "Changes to this policy",
  "changes.p":
    "If sync or other features that touch your data are introduced, this policy will be updated to describe them plainly.",
  "contact.h": "Contact",
  "contact.p":
    "If you have questions about this policy, reach us through the GitHub repository.",
  "contact.link": "Contact us on GitHub",
  "contact.email.pre": "Or email us at ",
  "contact.email.post": ".",
};

const ja: typeof en = {
  "meta.title": "プライバシーポリシー — Inkuna",
  "meta.description":
    "Inkuna のプライバシーポリシー。アプリは個人情報を収集しません — アカウントも、解析も、トラッキングもありません。",
  h1: "プライバシーポリシー",
  updated: "最終更新: 2026年8月17日",
  intro:
    "Inkuna は静かでプライベートなアプリとして設計されています。読書は個人的なもの。あなたの蔵書は、あなたのものです。",
  "summary.h": "要約",
  "summary.p":
    "Inkuna は個人情報を一切収集しません。アカウントも、解析も、広告も、トラッキングもありません。読んだデータはすべて端末の中だけに保存されます。",
  "collect.h": "収集するデータ",
  "collect.p":
    "ありません。Inkuna は完全にオフラインで動作する読書アプリです。蔵書・読書位置・設定は、端末の中にのみ保存されます。",
  "practices.h": "アカウントなし、広告なし、トラッキングなし",
  "practices.p":
    "Inkuna の利用にアカウントは不要です。データの販売・共有、広告の表示、第三者による解析・トラッキング SDK の利用は一切行いません。",
  "reading.h": "あなたの読書データ",
  "reading.p":
    "蔵書や読書位置が端末の外に出ることはありません。アップロードも、同期も、私たちがアクセスすることもありません。",
  "website.h": "ウェブサイト",
  "website.p":
    "Inkuna のウェブサイトは静的なページです。Cookie を設定せず、解析も、第三者のスクリプトの読み込みも行いません。",
  "changes.h": "本ポリシーの変更",
  "changes.p":
    "同期など、データに触れる機能を追加する際は、本ポリシーを更新して明確に説明します。",
  "contact.h": "お問い合わせ",
  "contact.p":
    "本ポリシーについてのご質問は、GitHub リポジトリからお問い合わせください。",
  "contact.link": "GitHub で問い合わせる",
  "contact.email.pre": "",
  "contact.email.post": " 宛にメールでもご連絡いただけます。",
};

const zh: typeof en = {
  "meta.title": "隐私政策 — Inkuna",
  "meta.description":
    "Inkuna 隐私政策 —— App 不收集任何个人信息：没有账号、没有分析、没有追踪。",
  h1: "隐私政策",
  updated: "最后更新：2026 年 8 月 17 日",
  intro:
    "Inkuna 的设计理念是安静与私密。阅读是私人的事，你的书库应当只属于你。",
  "summary.h": "概览",
  "summary.p":
    "Inkuna 不收集任何个人信息。没有账号、没有分析、没有广告、没有追踪。你阅读的一切都只保存在设备本地。",
  "collect.h": "我们收集的数据",
  "collect.p":
    "没有。Inkuna 是完全离线的阅读应用。书库、阅读进度与设置仅存储在你的设备上。",
  "practices.h": "没有账号、没有广告、没有追踪",
  "practices.p":
    "使用 Inkuna 无需账号。我们不出售或分享数据，不展示广告，也不使用任何第三方分析或追踪 SDK。",
  "reading.h": "你的阅读数据",
  "reading.p":
    "你的书库与阅读进度永远不会离开设备，不会被上传、同步，我们也无法访问。",
  "website.h": "网站",
  "website.p":
    "Inkuna 网站是纯静态页面，不设置 Cookie，不运行分析，也不加载任何第三方脚本。",
  "changes.h": "政策更新",
  "changes.p":
    "若未来引入同步等涉及数据的功能，本政策将随之更新并作出明确说明。",
  "contact.h": "联系我们",
  "contact.p": "对本政策有任何疑问，请通过 GitHub 仓库联系我们。",
  "contact.link": "在 GitHub 联系我们",
  "contact.email.pre": "也可发送邮件至 ",
  "contact.email.post": "。",
};

export const privacy: Dictionary<typeof en> = { en, ja, zh };
