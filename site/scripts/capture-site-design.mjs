/**
 * Browser screenshots and essential smoke checks for the minimal public site.
 * No Ax backend or Netlify deployment required.
 */
import puppeteer from "puppeteer";
import sharp from "sharp";
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
    ["home-wide", "/", 2048, 1018],
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
      if (hero.lines.join(' ') !== 'The context layer for AI agents' || !hero.font || hero.clipped ||
          hero.colors[1] !== hero.accent || hero.colors.slice(0, 1).some(color => color === hero.accent) ||
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
    // Pixel-level visual regression: ensure the overprint visibly changes on both desktop and mobile.
    // Animation existence alone previously missed an almost static 72/96-second movement.
    if (name === "home-desktop" || name === "home-mobile") {
      const clip = {
        x: Math.floor(width * 0.70),
        y: Math.floor(height * 0.20),
        width: width - Math.floor(width * 0.70),
        height: Math.floor(height * 0.55),
      };
      const seekMoire = async (milliseconds) => page.evaluate((milliseconds) => {
        document.querySelectorAll('.ax-halftone-dots, .ax-halftone-grid').forEach(el => {
          el.getAnimations().forEach(animation => {
            animation.pause();
            animation.currentTime = milliseconds;
          });
        });
      }, milliseconds);
      await seekMoire(0);
      const start = await sharp(await page.screenshot({ clip })).removeAlpha().raw().toBuffer();
      await seekMoire(10_000);
      const finish = await sharp(await page.screenshot({ clip })).removeAlpha().raw().toBuffer();
      const delta = start.reduce((sum, value, i) => sum + Math.abs(value - finish[i]), 0) / start.length;
      if (delta < 2.0) throw new Error(name + ': moiré not visibly moving (pixel delta ' + delta.toFixed(2) + ')');
      console.log(name, 'moiré visible motion, pixel delta', delta.toFixed(2));
      await page.evaluate(() => document.querySelectorAll('.ax-halftone-dots, .ax-halftone-grid').forEach(el =>
        el.getAnimations().forEach(animation => {
          animation.currentTime = Number(animation.effect.getTiming().duration) / 2;
          animation.play();
        })));
    }
    if (route !== "/" && name.startsWith("docs-")) {
      const docsTheme = await page.evaluate(() => ({
        wordmark: document.querySelector('.cg-site-title__wordmark')?.textContent?.trim(),
        moire: !!document.querySelector('.ax-doc-moire'),
        lime: getComputedStyle(document.documentElement).getPropertyValue('--ax-lime').trim(),
      }));
      if (docsTheme.wordmark !== 'Ax_' || !docsTheme.moire || docsTheme.lime !== '#b9ff6a') {
        throw new Error(name + ': docs did not inherit landing identity: ' + JSON.stringify(docsTheme));
      }
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
  if (!["The context layer", "for AI agents"].every(line => text.includes(line))) {
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
      moire.changed !== 2 || moire.moving !== 2 || !moire.mask.includes('linear-gradient') || moire.mask.includes('radial-gradient') || !moire.mask.includes('100%')) {
    throw new Error('Independent overprint interference and horizontal side fade required: ' + JSON.stringify(moire));
  }
  // Compare rendered pixels immediately before and after both animation wraps.
  // A one-tile translation should look continuous even though its matrix resets.
  await page.evaluate(() => document.querySelectorAll('.ax-halftone-dots').forEach(el =>
    el.getAnimations().forEach(animation => { animation.pause(); animation.currentTime = 4000; })));
  for (const selector of ['.ax-halftone-screen-two', '.ax-halftone-screen-three']) {
    const setPhase = async offset => page.evaluate(({ selector, offset }) => {
      const animation = document.querySelector(selector).getAnimations()[0];
      animation.currentTime = Number(animation.effect.getTiming().duration) + offset;
    }, { selector, offset });
    const clip = { x: 1000, y: 200, width: 400, height: 500 };
    await setPhase(-16);
    const before = await sharp(await page.screenshot({ clip })).removeAlpha().raw().toBuffer();
    await setPhase(16);
    const after = await sharp(await page.screenshot({ clip })).removeAlpha().raw().toBuffer();
    const difference = before.reduce((total, channel, index) => total + Math.abs(channel - after[index]), 0) / before.length;
    if (difference > 1.5) throw new Error(selector + ': visible loop discontinuity, mean pixel delta ' + difference);
    console.log(selector, 'continuous loop, mean pixel delta', difference.toFixed(3));
  }
  // Reload so the manual animation controls used by the pixel comparison do not
  // override CSS play state during normal interaction and reduced-motion checks.
  await page.reload({ waitUntil: 'networkidle0' });
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
  console.log("Minimal homepage checks passed: responsive viewport, CMY interference and side fade, native download links, and reduced motion.");
} finally {
  await browser?.close();
  server.kill();
}
