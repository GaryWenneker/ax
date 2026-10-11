/** Capture and check the public website, using the built site (no Ax backend required). */
import puppeteer from "puppeteer";
import { spawn } from "node:child_process";
import { mkdir, access } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = path.join(site, "public/screenshots/site-design");
await mkdir(out, { recursive: true });
const server = spawn(
  process.execPath,
  [
    "node_modules/astro/bin/astro.mjs",
    "preview",
    "--host",
    "127.0.0.1",
    "--port",
    "4422",
  ],
  { cwd: site, stdio: ["ignore", "pipe", "pipe"] },
);
await new Promise((resolve, reject) => {
  const timer = setTimeout(
    () => reject(new Error("Preview did not start")),
    30000,
  );
  server.on("error", reject);
  server.stdout.on("data", (data) => {
    if (data.toString().includes("4422")) {
      clearTimeout(timer);
      resolve();
    }
  });
  server.stderr.on("data", (data) => {
    if (data.toString().includes("Error")) console.error(data.toString());
  });
});
let browser;
try {
  browser = await puppeteer.launch({
    headless: process.platform === "linux" ? "shell" : true,
    pipe: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });
  const page = await browser.newPage();
  await page.setCacheEnabled(false);
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const shots = [
    ["home-desktop", "/", 1440, 1000],
    ["home-mobile", "/", 390, 844],
    ["docs-desktop", "/getting-started/introduction/", 1440, 1000],
    ["docs-mobile", "/getting-started/introduction/", 390, 844],
    ["replicas-desktop", "/guides/standalone-ax/", 1440, 1000],
  ];
  for (const [name, route, width, height] of shots) {
    await page.setViewport({ width, height, deviceScaleFactor: 1 });
    const response = await page.goto(`http://127.0.0.1:4422${route}`, {
      waitUntil: "networkidle0",
    });
    if (response.status() !== 200)
      throw new Error(`${route}: ${response.status()}`);
    await page.evaluate(async () => {
      await document.fonts.ready;
      document.documentElement.dataset.axMotion = "paused";
      await Promise.all(
        [...document.images].map((img) => {
          img.loading = "eager";
          return img.decode().catch(() => {});
        }),
      );
    });
    const check = await page.evaluate(() => ({
      overflow: document.documentElement.scrollWidth > innerWidth,
      images: [...document.images]
        .filter((x) => !x.complete || !x.naturalWidth)
        .map((x) => x.src),
      theme: document.documentElement.dataset.theme,
    }));
    if (check.overflow || check.images.length || check.theme !== "dark")
      throw new Error(`${name}: ${JSON.stringify(check)}`);
    await page.screenshot({
      path: path.join(out, `${name}.jpg`),
      fullPage: route === "/", quality: 85,
    });
    if (["home-desktop", "home-mobile", "docs-desktop", "docs-mobile"].includes(name))
      await page.screenshot({ path: path.join(out, `${name}-viewport.jpg`), quality: 85 });
    console.log(`${name}: no overflow, all images loaded, dark theme`);
  }
  // Every homepage internal guide target must exist in the static output.
  await page.goto("http://127.0.0.1:4422/", { waitUntil: "networkidle0" });
  const targets = await page.$$eval("a[href]", (links) => [
    ...new Set(
      links
        .map((x) => x.getAttribute("href"))
        .filter((x) => x.startsWith("/") && !x.includes("#")),
    ),
  ]);
  for (const target of targets)
    await access(path.join(site, "dist", target, "index.html"));
  await page.evaluate(() => { document.documentElement.dataset.axMotion = "running"; });
  await page.click("[data-ax-motion-toggle]");
  if ((await page.$eval("html", (x) => x.dataset.axMotion)) !== "paused")
    throw new Error("Pause control failed");
  const scene = await page.$$eval(".signal-ambient .ax-depth-spires", nodes => ({
    count: nodes.length,
    paused: nodes.every(x => x.getAnimations().every(a => a.playState === "paused")),
  }));
  if (scene.count !== 1 || !scene.paused)
    throw new Error(`Cinematic ray pause failed: ${JSON.stringify(scene)}`);
  const ticker = await page.$$eval(".ticker-group", nodes => nodes.length);
  if (ticker !== 2) throw new Error("The seamless integration ticker must render two matching halves");
  await page.emulateMediaFeatures([{ name: "prefers-reduced-motion", value: "reduce" }]);
  const respectsMotion = await page.$$eval(".signal-ambient .ax-depth-spires", nodes => nodes.every(x => x.getAnimations().every(a => a.playState === "paused")));
  if (!respectsMotion) throw new Error("Reduced motion not respected");
  if (errors.length) throw new Error(errors.join("\n"));
  console.log(
    `Homepage links, pause control, ${scene.count} cinematic rays, seamless ticker and reduced motion passed.`,
  );
} finally {
  await browser?.close();
  server.kill();
}
