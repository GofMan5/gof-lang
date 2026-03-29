import { defineDocs, defineConfig } from "fumadocs-mdx/config";
import gofGrammar from "../../tools/vscode-gof/syntaxes/gof.tmLanguage.json" with { type: "json" };

export const docs = defineDocs({
  dir: "content/docs",
});

export default defineConfig({
  mdxOptions: {
    rehypeCodeOptions: {
      themes: {
        light: "github-light",
        dark: "github-dark",
      },
      langs: [
        {
          ...gofGrammar,
          name: "gof",
        } as never,
      ],
    },
  },
});
