// @ts-check
import { defineConfig } from "astro/config";

export default defineConfig({
  site: "https://inkuna.app",
  i18n: {
    locales: ["en", "ja", "zh"],
    defaultLocale: "en",
    routing: {
      // every locale, English included, lives under its own prefix: /en/, /ja/, /zh/
      prefixDefaultLocale: true,
    },
  },
  // / served the English homepage before every locale was prefixed; keep it
  // working with a static 301 to the canonical /en/ home.
  redirects: {
    "/": { destination: "/en/", status: 301 },
  },
});
