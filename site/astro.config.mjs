import sitemap from "@astrojs/sitemap";
import { defineConfig } from "astro/config";

export default defineConfig({
  site: "https://jwilger.github.io",
  base: "/lanyard-ssh-agent",
  trailingSlash: "always",
  integrations: [sitemap()],
});
