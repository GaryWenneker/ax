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
  await page.setCacheEnabled(false);
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  const cases = [
    ["home-desktop", "/", 1440, 900],
    ["home-1280", "/", 1280, 720],
    ["home-1024", "/", 1024, 768],
    ["home-900", "/", 900, 700],
    ["home-tablet", "/", 768, 1024],
    ["home-mobile", "/", 390, 844],
    ["home-small-mobile", "/", 320, 640],
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
    if (route === "/") {
      const hero = await page.evaluate(() => {
        const lines = [...document.querySelectorAll('h1 > span')];
        const lime = getComputedStyle(document.body).getPropertyValue('--ax-lime').trim();
        const probe = document.createElement('span');
        probe.style.color = lime;
        document.body.append(probe);
        const accent = getComputedStyle(probe).color;
        probe.remove();
        return {
          lines: lines.map(el => el.textContent),
          colors: lines.map(el => getComputedStyle(el).color),
          accent,
          font: [...document.fonts].some(face => face.family === 'Oxanium Variable' && face.status === 'loaded') && getComputedStyle(document.querySelector('h1')).fontFamily.includes('Oxanium Variable'),
          clipped: [...lines, ...document.querySelectorAll('.ax-download-row > *, .ax-footer')].some(el => {
            const box = el.getBoundingClientRect();
            return box.left < 0 || box.right > innerWidth + 1 || (innerWidth >= 900 && box.bottom > innerHeight + 1);
          }),
          rows: lines.map(el => el.getBoundingClientRect().top),
        };
      });
      if (hero.lines.join(' ') !== 'Your context. Your rules. Your control.' || !hero.font || hero.clipped ||
          hero.colors[2] !== hero.accent || hero.colors.slice(0, 2).some(color => color === hero.accent) ||
          !hero.rows.every((top, i) => i === 0 || top > hero.rows[i - 1])) {
        throw new Error(name + ': incorrect slogan, font, accent or layout: ' + JSON.stringify(hero));
      }
      // Capture a representative phase of the independently moving screens.
      await page.evaluate(() => {
        document.querySelectorAll('.ax-halftone-dots').forEach(el => {
          el.getAnimations().forEach(animation => {
            animation.currentTime = Number(animation.effect.getTiming().duration) / 2;
          });
        });
      });
    }
    await page.screenshot({
      path: path.join(output, name + ".jpg"),
      fullPage: route === "/" && width < 900,
      quality: 85,
    });
    if (name === "home-desktop" || name === "home-mobile") {
      await page.screenshot({ path: path.join(output, name + "-viewport.jpg"), quality: 85 });
      await page.screenshot({ path: path.join(site, 'public/screenshots', name.replace('home-', 'home-install-') + '.png'), fullPage: width < 900 });
    }
    console.log(name, "pass", JSON.stringify(check));
  }
  await page.setViewport({ width: 1440, height: 900 });
  await page.goto("http://127.0.0.1:4422/", { waitUntil: "networkidle0" });
  const text = await page.$eval("h1", el => el.textContent);
  if (!["Your context.", "Your rules.", "Your control."].every(line => text.includes(line))) {
    throw new Error("Expected minimal homepage headline is missing");
  }
  if ((await page.$$(".signal-ambient, .ax-ares-architecture, .integrated-console")).length) {
    throw new Error("Old cinematic layout is still rendered");
  }
  const moire = await page.evaluate(() => {
    const field = document.querySelector('.ax-halftone');
    const screens = [...field.querySelectorAll('.ax-halftone-dots')];
    const before = screens.map(el => getComputedStyle(el).transform);
    screens.forEach(el => el.getAnimations().forEach(animation => { animation.currentTime += 8000; }));
    const after = screens.map(el => getComputedStyle(el).transform);
    return {
      count: screens.length,
      pitch: screens.map(el => getComputedStyle(el).backgroundSize),
      blending: screens.every(el => getComputedStyle(el).mixBlendMode === 'multiply'),
      mask: getComputedStyle(field).maskImage,
      hardCircle: getComputedStyle(field.querySelector('.ax-halftone-grid')).borderRadius !== '0px',
      changed: after.filter((transform, index) => transform !== before[index]).length,
      moving: screens.filter(el => el.getAnimations().some(animation => animation.playState === 'running')).length,
    };
  });
  if (moire.count !== 3 || new Set(moire.pitch).size !== 3 || !moire.blending || moire.hardCircle ||
      moire.changed !== 2 || moire.moving !== 2 || !moire.mask.includes('closest-side') || !moire.mask.includes('100%')) {
    throw new Error('Independent overprint interference and feathered edges required: ' + JSON.stringify(moire));
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
  const respectsMotion = await page.$$eval(".ax-halftone-dots", elements =>
    elements.every(el => el.getAnimations().every(a => a.playState !== "running")));
  if (!respectsMotion) throw new Error("Reduced-motion preference ignored");
  const targets = await page.$$eval('a[href]', links =>
    [...new Set(links.map(x => x.getAttribute("href")).filter(x => x.startsWith("/") && !x.includes("#")))]);
  for (const target of targets) await access(path.join(site, "dist", target, "index.html"));
  if (errors.length) throw new Error(errors.join("\n"));
  console.log("Minimal homepage checks passed: responsive viewport, CMY interference and edge fade, native download links, and reduced motion.");
} finally {
  await browser?.close();
  server.kill();
}
