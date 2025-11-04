import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts"], // or "src/index.ts"
  format: ["esm"], // can add "cjs" if needed
  dts: true, // generates type definitions
  outDir: "dist",
  clean: true, // remove old dist/ before building
  sourcemap: true,
  target: "es2020",
  splitting: false, // keep single files (optional)
  skipNodeModulesBundle: true, // don't bundle deps
  esbuildOptions(options) {
    options.conditions = ["import", "module", "default"];
  },
});
