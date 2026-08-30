import { fileURLToPath } from "node:url";
import { gvyaContentPlugin } from "./content-host.mjs";

export default {
  plugins: [gvyaContentPlugin(fileURLToPath(new URL("../../content", import.meta.url)))],
  build: {
    outDir: "web-dist",
    emptyOutDir: true,
  },
};
