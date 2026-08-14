// @ts-check
import { defineConfig } from "astro/config";

export default defineConfig({
  site: "https://inkuna.app",
  i18n: {
    locales: ["en", "ja", "zh"],
    defaultLocale: "en",
    // default locale stays unprefixed: / is English, /ja/ and /zh/ localized
  },
});
