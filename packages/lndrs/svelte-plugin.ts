import { plugin } from "bun";
import { compileModule } from "svelte/compiler";

const transpiler = new Bun.Transpiler({ loader: "ts" });

plugin({
  name: "svelte-runes",
  setup(build) {
    build.onLoad({ filter: /\.svelte\.ts$/ }, async ({ path }) => {
      const source = await Bun.file(path).text();
      const javascript = transpiler.transformSync(source);
      const compiled = compileModule(javascript, {
        filename: path,
        generate: "client",
        dev: process.env.NODE_ENV !== "production",
      });

      return { contents: compiled.js.code, loader: "js" };
    });
  },
});
