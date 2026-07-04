import type { AstroIntegration } from "astro";

export default function ogImages(): AstroIntegration {
  return {
    name: "thndrs-og-images",
    hooks: {
      "astro:config:setup": ({ injectRoute }) => {
        injectRoute({ pattern: "/og.png", entrypoint: new URL("./endpoint.ts", import.meta.url), prerender: true });
      },
    },
  };
}
