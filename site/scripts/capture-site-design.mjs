/**
 * Browser screenshots and essential smoke checks for the minimal public site.
 * No Ax backend or Netlify deployment required.
 */
import puppeteer from "puppeteer";
import { spawn } from "node:child_process";
import { mkdir, access } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(site, "public/screenshots/site-design");
await mkdir(output, { recursive: true });
const server = spawn(process.execPath,
  ["node_modules/astro/bin/astro.mjs", "preview", "--host", "127.0.0.1", "--port", "4422"],
  { cwd: site, stdio: ["ignore", "pipe", "pipe"] });
let browser;
try {
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Astro preview did not start")), 30000);
    server.once("error", reject);
    server.stdout.on("data", bytes => {
      if (bytes.toString().includes("4422")) { clearTimeout(timeout); resolve(); }
    });
    server.stderr.on("data", bytes => {
      if (/Error/i.test(bytes.toString())) console.error(bytes.toString());
    });
  });
  browser = await puppeteer.launch({
    headless: process.platform === "linux" ? "shell" : true,
    pipe: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  const cases = [
    ["home-desktop", "/", 1440, 900],
    ["home-1280", "/", 1280, 720],
    ["home-1024", "/", 1024, 768],
    ["home-mobile", "/", 390, 844],
    ["docs-desktop", "/getting-started/introduction/", 1440, 900],
    ["docs-mobile", "/getting-started/introduction/", 390, 844],
    ["replicas-desktop", "/guides/standalone-ax/", 1440, 900],
  ];
  for (const [name, route, width, height] of cases) {
    await page.setViewport({ width, height, deviceScaleFactor: 1 });
    const response = await page.goto("http://127.0.0.1:4422" + route, { waitUntil: "networkidle0" });
    if (response.status() !== 200) throw new Error(route + ": HTTP " + response.status());
    await page.evaluate(async () => {
      await document.fonts.ready;
      await Promise.all([...document.images].map(async img => {
        img.loading = "eager";
        try { await img.decode(); } catch {}
      }));
    });
    const check = await page.evaluate(() => ({
      horizontalOverflow: document.documentElement.scrollWidth > innerWidth,
      verticalOverflow: document.documentElement.scrollHeight > innerHeight,
      dark: document.documentElement.dataset.theme === "dark",
      brokenImages: [...document.images].filter(x => !x.complete || !x.naturalWidth).map(x => x.src),
    }));
    if (check.horizontalOverflow || !check.dark || check.brokenImages.length) {
      throw new Error(name + ": " + JSON.stringify(check));
    }
    if (route === "/" && width >= 900 && check.verticalOverflow) {
      throw new Error(name + ": desktop homepage must fit the viewport");
    }
    await page.screenshot({
      path: path.join(output, name + ".jpg"),
      fullPage: route === "/" && width < 900,
      quality: 85,
    });
    if (name === "home-desktop" || name === "home-mobile") {
      await page.screenshot({ path: path.join(output, name + "-viewport.jpg"), quality: 85 });
    }
    console.log(name, "pass", JSON.stringify(check));
  }
  await page.setViewport({ width: 1440, height: 900 });
  await page.goto("http://127.0.0.1:4422/", { waitUntil: "networkidle0" });
  const text = await page.$eval("h1", el => el.textContent);
  if (text.trim() !== "Your context.Your rules.Your control.") {
    throw new Error("Approved homepage slogan is missing: " + text);
  }
  const typography = await page.evaluate(() => {
    const h1 = document.querySelector(".ax-headline");
    const children = [...h1.querySelectorAll(":scope > span")];
    const body = getComputedStyle(document.body);
    const root = getComputedStyle(h1);
    const styles = children.map(el => getComputedStyle(el).color);
    const lime = body.getPropertyValue("--ax-lime").trim();
    const loadedFont = [...document.fonts].some(face =>
      face.family === "Oxanium Variable" && face.status === "loaded");
    return { fontFamily: root.fontFamily, loadedFont, styles, lime,
      childText: children.map(el => el.textContent) };
  });
  if (!typography.loadedFont || !typography.fontFamily.includes("Oxanium Variable"))
    throw new Error("Bundled Oxanium Variable did not load: " + JSON.stringify(typography));
  if (typography.styles.length !== 3 || typography.styles[0] !== typography.styles[1] ||
      typography.styles[0] === typography.styles[2])
    throw new Error("Only 'Your control.' should carry lime: " + JSON.stringify(typography));
  if ((await page.$$(".signal-ambient, .ax-ares-architecture, .integrated-console")).length) {
    throw new Error("Old cinematic layout is still rendered");
  }
  const moire = await page.$$eval(".ax-moire-layer", elements => ({
    count: elements.length,
    moving: elements.some(el => el.getAnimations().some(a => a.playState === "running")),
  }));
  if (moire.count !== 2 || !moire.moving) {
    throw new Error("Two animated moire layers required: " + JSON.stringify(moire));
  }
  await page.click("[data-ax-platform-toggle]");
  await page.click('[data-ax-os="windows"]');
  await page.click('[data-ax-arch="arm64"]');
  const winUrl = await page.$eval("[data-ax-download]", el => el.getAttribute("href"));
  if (!winUrl.endsWith("ax-win32-arm64.zip")) throw new Error("Windows ARM64 asset mismatch");
  await page.click('[data-ax-os="macos"]');
  await page.click('[data-ax-arch="x64"]');
  const macUrl = await page.$eval("[data-ax-download]", el => el.getAttribute("href"));
  if (!macUrl.endsWith("ax-darwin-x64.tar.gz")) throw new Error("Intel macOS asset mismatch");
  await page.emulateMediaFeatures([{ name: "prefers-reduced-motion", value: "reduce" }]);
  const respectsMotion = await page.$$eval(".ax-moire-layer", elements =>
    elements.every(el => el.getAnimations().every(a => a.playState !== "running")));
  if (!respectsMotion) throw new Error("Reduced-motion preference ignored");
  const targets = await page.$$eval('a[href]', links =>
    [...new Set(links.map(x => x.getAttribute("href")).filter(x => x.startsWith("/") && !x.includes("#")))]);
  for (const target of targets) await access(path.join(site, "dist", target, "index.html"));
  if (errors.length) throw new Error(errors.join("\n"));
  console.log("Minimal homepage checks passed: responsive viewport, moire, native download links, and reduced motion.");
} finally {
  await browser?.close();
  server.kill();
}
