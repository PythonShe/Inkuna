import type { Dictionary } from "./utils";

const en = {
  source: "Source",
  license: "License",
  support: "Support",
  privacy: "Privacy",
  lang: "Language",
};

const ja: typeof en = {
  source: "ソースコード",
  license: "ライセンス",
  support: "サポート",
  privacy: "プライバシー",
  lang: "言語",
};

const zh: typeof en = {
  source: "源代码",
  license: "许可证",
  support: "支持",
  privacy: "隐私",
  lang: "语言",
};

export const footer: Dictionary<typeof en> = { en, ja, zh };
