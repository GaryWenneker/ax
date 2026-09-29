import esbuild from "esbuild";

const prod = process.argv.includes("--production");

await esbuild.build({
  entryPoints: ["src/main.ts"],
  bundle: true,
  outfile: "main.js",
  format: "cjs",
  platform: "browser",
  target: "es2022",
  external: ["obsidian"],
  sourcemap: prod ? false : "inline",
  minify: prod,
  logLevel: "info",
});
