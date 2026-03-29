import { docs } from "collections/index";
import { loader } from "fumadocs-core/source";
import { i18n } from "@/lib/i18n";

const mdxSource = docs.toFumadocsSource();
const files =
  typeof mdxSource.files === "function"
    ? (mdxSource.files as () => typeof mdxSource.files)()
    : mdxSource.files;

export const source = loader({
  i18n,
  baseUrl: "/docs",
  source: { files } as typeof mdxSource,
});
