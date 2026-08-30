// Chrome DevTools Protocol acceptance runner for the production Studio bundle.
import { createReadStream, existsSync } from "node:fs";
import { cp, mkdtemp, rm, stat } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { delimiter, extname, isAbsolute, join, normalize, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { createServer as createNetServer } from "node:net";
import { readContentSnapshot, STUDIO_CONTENT_API, writeContentSnapshot } from "../apps/studio/content-host.mjs";

const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const WEB_ROOT = resolve(ROOT, "apps/studio/web-dist");
const INDEX = resolve(WEB_ROOT, "index.html");
const DEFAULT_STANDARD_PACKAGE_ID = "core.smalltalk.formal";
const DEFAULT_STANDARD_BEHAVIOR_ID = `${DEFAULT_STANDARD_PACKAGE_ID}.hello.behavior`;

function fail(message) { throw new Error(message); }
function sleep(ms) { return new Promise((resolvePromise) => setTimeout(resolvePromise, ms)); }

async function freePort() {
  return await new Promise((resolvePromise, reject) => {
    const server = createNetServer();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") return reject(new Error("failed to allocate local port"));
      const port = address.port;
      server.close((error) => error ? reject(error) : resolvePromise(port));
    });
  });
}

function mime(path) {
  return ({
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".svg": "image/svg+xml",
    ".wasm": "application/wasm",
  })[extname(path)] ?? "application/octet-stream";
}

async function staticServer(contentRoot) {
  if (!existsSync(INDEX)) fail("Studio Vite output missing; run `npm run build:studio` first");
  const writes = { started: 0, completed: 0 };
  const server = createServer(async (request, response) => {
    try {
      const rawPath = decodeURIComponent(new URL(request.url ?? "/", "http://127.0.0.1").pathname);
      if (rawPath === STUDIO_CONTENT_API) {
        if (request.method === "PUT") writes.started += 1;
        const value = request.method === "GET"
          ? await readContentSnapshot(contentRoot)
          : request.method === "PUT"
            ? await writeContentSnapshot(contentRoot, JSON.parse(Buffer.concat(await Array.fromAsync(request)).toString("utf8")))
            : null;
        if (request.method === "PUT") writes.completed += 1;
        if (value === null) { response.writeHead(405, { allow: "GET, PUT" }); response.end(); return; }
        response.writeHead(200, { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" });
        response.end(JSON.stringify(value));
        return;
      }
      const candidate = resolve(WEB_ROOT, `.${normalize(rawPath)}`);
      const relativePath = relative(WEB_ROOT, candidate);
      const safe = relativePath === "" || (!relativePath.startsWith("..") && !isAbsolute(relativePath));
      if (!safe) { response.writeHead(403); response.end("forbidden"); return; }
      let file = candidate;
      try {
        const info = await stat(file);
        if (info.isDirectory()) file = INDEX;
      } catch {
        file = INDEX;
      }
      response.writeHead(200, { "content-type": mime(file), "cache-control": "no-store" });
      createReadStream(file).pipe(response);
    } catch (error) {
      response.writeHead(500); response.end(String(error));
    }
  });
  server.autosaveWrites = writes;
  await new Promise((resolvePromise, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolvePromise);
  });
  const address = server.address();
  if (!address || typeof address === "string") fail("static server did not expose a TCP port");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

function browserExecutable() {
  const windowsCandidates = process.platform === "win32" ? [
    join(process.env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)", "Microsoft", "Edge", "Application", "msedge.exe"),
    join(process.env.ProgramFiles ?? "C:\\Program Files", "Microsoft", "Edge", "Application", "msedge.exe"),
    join(process.env.ProgramFiles ?? "C:\\Program Files", "Google", "Chrome", "Application", "chrome.exe"),
  ] : [];
  const candidates = [
    process.env.GVYA_BROWSER,
    ...windowsCandidates,
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
  ].filter(Boolean);
  const pathEntries = (process.env.PATH ?? "").split(delimiter);
  for (const candidate of candidates) {
    if (isAbsolute(candidate)) { if (existsSync(candidate)) return candidate; continue; }
    for (const entry of pathEntries) {
      const full = join(entry, candidate);
      if (existsSync(full)) return full;
    }
  }
  fail("Chromium/Chrome not found; set GVYA_BROWSER to an executable path");
}

class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
  }
  async open() {
    await new Promise((resolvePromise, reject) => {
      this.ws.addEventListener("open", resolvePromise, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
    this.ws.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(`${message.error.code}: ${message.error.message}`));
        else pending.resolve(message.result ?? {});
        return;
      }
      for (const listener of this.listeners.get(message.method) ?? []) listener(message);
    });
  }
  send(method, params = {}, sessionId = undefined) {
    const id = this.nextId++;
    const payload = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;
    return new Promise((resolvePromise, reject) => {
      this.pending.set(id, { resolve: resolvePromise, reject });
      this.ws.send(JSON.stringify(payload));
    });
  }
  on(method, listener) {
    const rows = this.listeners.get(method) ?? [];
    rows.push(listener);
    this.listeners.set(method, rows);
  }
  close() { this.ws.close(); }
}

async function pollJson(url, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return await response.json();
      last = new Error(`HTTP ${response.status}`);
    } catch (error) { last = error; }
    await sleep(100);
  }
  throw last ?? new Error(`timed out waiting for ${url}`);
}

/**
 * Resolves once every started autosave PUT has completed and the Project the test just created is
 * on disk. This observes the persistence contract the section asserts, with no timing assumption.
 */
async function waitForAutosave(server, contentRoot, timeoutMs = 15000) {
  const writes = server.autosaveWrites;
  const target = join(contentRoot, "projects", "browser-project", "project.json");
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (writes.completed > 0 && writes.started === writes.completed && existsSync(target)) {
      // One more settle pass: a later debounce may still be queued behind this one.
      const settled = writes.completed;
      await sleep(150);
      if (writes.started === writes.completed && writes.completed === settled) return;
      continue;
    }
    await sleep(50);
  }
  fail(`autosave did not persist ${target} (started=${writes.started} completed=${writes.completed})`);
}

async function waitFor(cdp, sessionId, expression, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  let attempt = 0;
  while (Date.now() < deadline) {
    const result = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
    if (result.result?.value) return result.result.value;
    if (process.env.GVYA_BROWSER_DEBUG && attempt++ % 10 === 0) console.error("CDP wait", expression, JSON.stringify(result));
    await sleep(100);
  }
  fail(`browser condition timed out: ${expression}`);
}

async function evalValue(cdp, sessionId, expression) {
  const result = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
  if (result.exceptionDetails) fail(`browser evaluation failed: ${result.exceptionDetails.text ?? expression}`);
  return result.result?.value;
}

async function main() {
  const temporaryRoot = await mkdtemp(join(tmpdir(), "gvya-browser-"));
  const contentRoot = join(temporaryRoot, "content");
  await cp(resolve(ROOT, "content"), contentRoot, { recursive: true });
  const { server, url } = await staticServer(contentRoot);
  const debugPort = await freePort();
  const profile = join(temporaryRoot, "profile");
  const executable = browserExecutable();
  const browser = spawn(executable, [
    "--headless=new",
    "--no-sandbox",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    `--remote-debugging-port=${debugPort}`,
    "--remote-allow-origins=*",
    `--user-data-dir=${profile}`,
    "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"] });
  let browserStderr = "";
  browser.stderr.on("data", (chunk) => { browserStderr += String(chunk); });

  let cdp;
  try {
    const version = await pollJson(`http://127.0.0.1:${debugPort}/json/version`);
    cdp = new Cdp(version.webSocketDebuggerUrl);
    await cdp.open();
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const consoleErrors = [];
    const runtimeErrors = [];
    cdp.on("Runtime.exceptionThrown", (event) => { if (event.sessionId === sessionId) { const details=event.params?.exceptionDetails; runtimeErrors.push(details?.exception?.description ?? details?.text ?? "runtime exception"); } });
    cdp.on("Log.entryAdded", (event) => {
      if (event.sessionId === sessionId && ["error", "warning"].includes(event.params?.entry?.level)) consoleErrors.push(`${event.params.entry.level}: ${event.params.entry.text}`);
    });
    await cdp.send("Page.enable", {}, sessionId);
    await cdp.send("Runtime.enable", {}, sessionId);
    await cdp.send("Log.enable", {}, sessionId);
    const navResult = await cdp.send("Page.navigate", { url }, sessionId);
    if (navResult.errorText) fail(`browser navigation failed: ${navResult.errorText}`);
    if (process.env.GVYA_BROWSER_DEBUG) {
      console.error("Page.navigate", JSON.stringify(navResult));
      await sleep(500);
      console.error("Page state", JSON.stringify(await cdp.send("Runtime.evaluate", { expression: `({href:location.href,state:document.readyState,body:document.body?.innerHTML?.slice(0,300)})`, returnByValue: true }, sessionId)));
    }
    await sleep(250);
    if (runtimeErrors.length) fail(`browser startup runtime exceptions: ${runtimeErrors.join(" | ")}`);
    if (consoleErrors.length) fail(`browser startup console/log errors: ${consoleErrors.join(" | ")}`);
    await waitFor(cdp, sessionId, `document.readyState === "complete" && Boolean(document.querySelector(".app-shell"))`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.project-row .row-title')].some(x=>x.textContent.trim()==='GVYA project')`);

    const nav = await evalValue(cdp, sessionId, `[...document.querySelectorAll(".nav-button")].map(x => x.textContent.trim())`);
    const expected = ["Projects", "Shared Packages", "Settings"];
    if (JSON.stringify(nav) !== JSON.stringify(expected)) fail(`global navigation must stay minimal: ${JSON.stringify(nav)}`);
    const forbiddenChrome = await evalValue(cdp, sessionId, `[...document.querySelectorAll(".topbar button")].some(x => ["Undo","Redo","Save","Open"].includes(x.textContent.trim()) || x.textContent.includes("History"))`);
    if (forbiddenChrome) fail("global topbar contains legacy workspace controls");

    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.nav-button')].find(x=>x.textContent.trim()==='Projects');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `location.hash==='#/projects' && document.querySelector('.page-heading h1')?.textContent==='Projects'`);

    // Project creation/editing is modal, blocking, movable, and row-based.
    if (!await evalValue(cdp, sessionId, `document.querySelectorAll(".project-row").length > 0`)) fail("Projects are not rendered as rows");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()==="New project"); b?.click(); return Boolean(b); })()`);
    await waitFor(cdp, sessionId, `Boolean(document.querySelector('.modal-card[role="dialog"][aria-modal="true"]'))`);
    const modalContract = await evalValue(cdp, sessionId, `(() => {
      const modal=document.querySelector('.modal-card'), head=document.querySelector('.modal-head'), backdrop=document.querySelector('.modal-backdrop');
      if(!modal||!head||!backdrop||!modal.contains(document.activeElement)||document.querySelectorAll('[inert]').length===0)return false;
      backdrop.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));
      if(!document.querySelector('.modal-card'))return false;
      head.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,pointerId:7,clientX:100,clientY:100}));
      head.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,pointerId:7,clientX:140,clientY:130}));
      head.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerId:7,clientX:140,clientY:130}));
      return getComputedStyle(head).cursor==='move';
    })()`);
    if (!modalContract) fail("system modal is not blocking/focus-contained/draggable");
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.style.transform !== 'translate(0px, 0px)'`);
    // Toasts are transient system feedback and must remain visible/interactable above an open modal.
    const transientLayerContract = await evalValue(cdp, sessionId, `(() => {
      const modal=document.querySelector('.modal-backdrop');
      const transientRule=[...document.styleSheets].flatMap(sheet=>{try{return [...sheet.cssRules]}catch{return []}}).find(rule=>rule.selectorText==='.toast');
      const z=transientRule ? Number(String(transientRule.style.zIndex||'0')) : 0;
      const modalZ=modal ? Number(getComputedStyle(modal).zIndex||'0') : 0;
      return z>modalZ;
    })()`);
    if (!transientLayerContract) fail("toast layer is not above the modal layer");
    const fieldLabelContract = await evalValue(cdp, sessionId, `(() => {
      const label=document.querySelector('.field > span');
      if(!label)return false;
      const style=getComputedStyle(label);
      return parseFloat(style.fontSize)>=13 && Number(style.fontWeight)>=600 && style.color!==getComputedStyle(document.body).backgroundColor;
    })()`);
    if (!fieldLabelContract) fail("input labels are too small or low-contrast");
    const fillProject = await evalValue(cdp, sessionId, `(() => {
      const field=(label)=>[...document.querySelectorAll('.modal-card label')].find(x=>x.textContent.trim().startsWith(label))?.querySelector('input,textarea');
      const set=(e,v)=>{const proto=e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,'value').set.call(e,v);e.dispatchEvent(new Event('input',{bubbles:true}));};
      const id=field('Project ID'), title=field('Name'), desc=field('Description'); if(!id||!title||!desc)return false; set(id,'browser-project');set(title,'Browser Project');set(desc,'Browser acceptance project');return true;
    })()`);
    if (!fillProject) fail("Project modal fields missing");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card button')].find(x=>x.textContent.trim()==='Save'); b?.click(); return Boolean(b); })()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.project-row .row-title')].some(x=>x.textContent.trim()==='Browser Project')`);
    if (!await evalValue(cdp, sessionId, `Boolean(document.querySelector('.toast'))`)) fail("success toast was not rendered before auto-hide");
    await sleep(2600);
    if (await evalValue(cdp, sessionId, `Boolean(document.querySelector('.toast'))`)) fail("success toast did not auto-hide");

    // Row title opens Project. Project Bots/Packages are real browser-history locations, not hidden component state.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.project-row .row-title')].find(x=>x.textContent.trim()==='Browser Project'); b?.click(); return Boolean(b); })()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.breadcrumb button,.breadcrumb strong')].some(x=>x.textContent.trim()==='Browser Project') && [...document.querySelectorAll('.local-tabs button')].some(x=>x.textContent.trim()==='Bots')`);
    const projectTabs = await evalValue(cdp, sessionId, `[...document.querySelectorAll('.local-tabs button')].map(x=>x.textContent.trim())`);
    if (JSON.stringify(projectTabs) !== JSON.stringify(["Bots","Packages"])) fail("Project tabs are not Bots/Packages only");
    if (!await evalValue(cdp, sessionId, `location.hash.endsWith('/bots')`)) fail("Project Bots tab has no browser navigation location");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.local-tabs button')].find(x=>x.textContent.trim()==='Packages');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `location.hash.endsWith('/packages') && [...document.querySelectorAll('.local-tabs button')].find(x=>x.textContent.trim()==='Packages')?.classList.contains('active')`);
    await evalValue(cdp, sessionId, `history.back()`);
    await waitFor(cdp, sessionId, `location.hash.endsWith('/bots') && [...document.querySelectorAll('.local-tabs button')].find(x=>x.textContent.trim()==='Bots')?.classList.contains('active')`);
    await evalValue(cdp, sessionId, `history.forward()`);
    await waitFor(cdp, sessionId, `location.hash.endsWith('/packages') && [...document.querySelectorAll('.local-tabs button')].find(x=>x.textContent.trim()==='Packages')?.classList.contains('active')`);

    // Project Packages is ownership only: no Shared attachment and no Project-level Override surface.
    if (await evalValue(cdp, sessionId, `[...document.querySelectorAll('button')].some(x=>x.textContent.trim()==='Add shared package')`)) fail("Project Packages still exposes Shared Package attachment");
    if (await evalValue(cdp, sessionId, `[...document.querySelectorAll('.panel h2')].some(x=>x.textContent.trim()==='Shared Packages')`)) fail("Project Packages still embeds a Shared Packages section");
    if (await evalValue(cdp, sessionId, `[...document.querySelectorAll('.project-package-columns button')].some(x=>x.textContent.trim()==='Override')`)) fail("Project Packages still exposes Project-level Override");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='New project package');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.textContent.includes('New project package')`);
    await evalValue(cdp, sessionId, `(() => { const field=[...document.querySelectorAll('.modal-card label')].find(x=>x.textContent.trim().startsWith('Package ID'))?.querySelector('input');if(!field)return false;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,'browser-project-package');field.dispatchEvent(new Event('input',{bubbles:true}));return true;})()`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card button')].find(x=>x.textContent.trim()==='Save');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.project-package-columns .row-title')].some(x=>x.textContent.trim()==='browser-project-package')`);
    if (!await evalValue(cdp, sessionId, `[...document.querySelectorAll('.page-stack > section.panel h2')].some(x=>x.textContent.trim()==='Project Fallback Packages')`)) fail("Project Packages does not expose its Project Fallback Packages section");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='New project fallback package');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.textContent.includes('New project fallback package')`);
    await evalValue(cdp, sessionId, `(() => { const field=[...document.querySelectorAll('.modal-card label')].find(x=>x.textContent.trim().startsWith('Package ID'))?.querySelector('input');if(!field)return false;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,'browser-project-fallback');field.dispatchEvent(new Event('input',{bubbles:true}));return true;})()`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card button')].find(x=>x.textContent.trim()==='Save');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.project-package-columns .row-title')].some(x=>x.textContent.trim()==='browser-project-fallback')`);

    await evalValue(cdp, sessionId, `history.back()`);
    await waitFor(cdp, sessionId, `location.hash.endsWith('/bots') && [...document.querySelectorAll('.local-tabs button')].find(x=>x.textContent.trim()==='Bots')?.classList.contains('active')`);

    // Bot add/edit is modal; opening a Bot exposes only contextual Bot tabs.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='New bot'); b?.click(); return Boolean(b); })()`);
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.textContent.includes('New bot')`);
    await evalValue(cdp, sessionId, `(() => { const field=(label)=>[...document.querySelectorAll('.modal-card label')].find(x=>x.textContent.trim().startsWith(label))?.querySelector('input,textarea'); const set=(e,v)=>{const proto=e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,'value').set.call(e,v);e.dispatchEvent(new Event('input',{bubbles:true}));}; const id=field('Bot ID'), title=field('Name'); if(!id||!title)return false;set(id,'test-bot');set(title,'Test Bot');return true; })()`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card button')].find(x=>x.textContent.trim()==='Save');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.bot-list-columns .row-title')].some(x=>x.textContent.trim()==='Test Bot')`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.bot-list-columns .row-title')].find(x=>x.textContent.trim()==='Test Bot');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `Boolean(document.querySelector('.context-tabs'))`);
    const botTabs = await evalValue(cdp, sessionId, `[...document.querySelectorAll('.context-tabs button')].map(x=>x.textContent.trim())`);
    const expectedBotTabs=["Overview","Packages","Simulate","Settings","Build"];
    if (JSON.stringify(botTabs)!==JSON.stringify(expectedBotTabs)) fail(`Bot contextual tabs mismatch: ${JSON.stringify(botTabs)}`);
    if (!await evalValue(cdp, sessionId, `document.querySelector('.page-heading h1')?.textContent==='Test Bot' && document.body.textContent.includes('Overview')`)) fail("Bot did not open on Overview");
    if (!await evalValue(cdp, sessionId, `[...document.querySelectorAll('.page-heading button')].some(x=>x.textContent.trim()==='Download bot ZIP')`)) fail("Bot Overview does not expose its explicit folder ZIP download");
    const crumb = await evalValue(cdp, sessionId, `[...document.querySelectorAll('.breadcrumb button,.breadcrumb strong')].map(x=>x.textContent.trim())`);
    for (const label of ["Projects","Browser Project","Test Bot"]) if (!crumb.includes(label)) fail(`breadcrumb missing ${label}`);
    if (crumb.includes("Overview")) fail("context tab leaked into breadcrumb");

    // Bot package composition is a dedicated list. Shared choices remain live references with explicit provenance.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Packages');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.page-heading h1')?.textContent==='Packages' && Boolean(document.querySelector('.bot-override-columns .row-title'))`);
    if (!await evalValue(cdp, sessionId, `document.body.textContent.includes('Bot Package') && document.body.textContent.includes('Added Packages')`)) fail("Bot Packages does not separate the structural Bot Package from added Packages");
    const botPackageSections = await evalValue(cdp, sessionId, `[...document.querySelectorAll('.page-stack > section.panel h2')].map(x=>x.textContent.trim())`);
    if (JSON.stringify(botPackageSections)!==JSON.stringify(["Bot Package","Added Packages","Fallback Package"])) fail(`Bot Package sections are out of order: ${JSON.stringify(botPackageSections)}`);
    if (await evalValue(cdp, sessionId, `[...document.querySelectorAll('.bot-override-columns .row-actions button')].some(x=>/remove|delete/i.test(x.getAttribute('aria-label')||x.textContent))`)) fail("Structural Bot Package exposes an independent delete action");
    if (await evalValue(cdp, sessionId, `[...document.querySelectorAll('button')].some(x=>x.textContent.trim()==='New bot package')`)) fail("Bot Packages still exposes multiple Bot-owned Package creation");

    // Manage Bot Package membership as a checkbox list across Shared and Project-owned Packages.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='Manage Packages');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.textContent.includes('Manage Packages')`);
    if (await evalValue(cdp, sessionId, `Boolean(document.querySelector('.modal-card select'))`)) fail("Manage Packages still uses a confusing dropdown");
    const managedGroups = await evalValue(cdp, sessionId, `[...document.querySelectorAll('.modal-card .package-manager-groups h3')].map(x=>x.textContent.trim())`);
    if (JSON.stringify(managedGroups)!==JSON.stringify(["Shared Packages","Project Packages"])) fail(`Manage Packages groups mismatch: ${JSON.stringify(managedGroups)}`);
    const selectedBoth = await evalValue(cdp, sessionId, `(() => { const want=new Set(['${DEFAULT_STANDARD_PACKAGE_ID}','browser-project-package']);let found=0;for(const row of document.querySelectorAll('.modal-card .package-choice-row')){const id=row.querySelector('strong')?.textContent.trim();const input=row.querySelector('input[type=checkbox]');if(id&&want.has(id)&&input){if(!input.checked)input.click();found++;}}return found===2;})()`);
    if (!selectedBoth) fail("Manage Packages does not expose both Shared and Project Packages as checkbox choices");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card button')].find(x=>x.textContent.trim()==='Save changes');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.data-row.bot-package-columns')].some(x=>x.textContent.includes('${DEFAULT_STANDARD_PACKAGE_ID}')) && [...document.querySelectorAll('.data-row.bot-package-columns')].some(x=>x.textContent.includes('browser-project-package'))`);
    const fallbackGroups = await evalValue(cdp, sessionId, `[...document.querySelectorAll('select optgroup')].map(x=>x.label)`);
    if (!fallbackGroups.includes("Project Fallback Packages")) fail(`Bot fallback selector does not group Project Fallback Packages: ${JSON.stringify(fallbackGroups)}`);
    if (!await evalValue(cdp, sessionId, `(() => { const select=[...document.querySelectorAll('select')].find(x=>[...x.options].some(o=>o.value==='browser-project-fallback'));if(!select)return false;Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(select,'browser-project-fallback');select.dispatchEvent(new Event('change',{bubbles:true}));return true;})()`)) fail("Bot fallback selector does not expose the Project Fallback Package");
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('select')].some(x=>x.value==='browser-project-fallback') && [...document.querySelectorAll('button')].some(x=>x.textContent.trim()==='Open Fallback Package')`);

    // A selected Shared Package can be overridden into the one Bot Package without copying its source.
    await evalValue(cdp, sessionId, `(() => { const row=[...document.querySelectorAll('.data-row.bot-package-columns')].find(x=>x.textContent.includes('${DEFAULT_STANDARD_PACKAGE_ID}'));const b=row&&[...row.querySelectorAll('button')].find(x=>x.textContent.trim()==='Override');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.textContent.includes('Override content · ${DEFAULT_STANDARD_PACKAGE_ID}')`);
    await evalValue(cdp, sessionId, `(() => { const row=[...document.querySelectorAll('.modal-card .inline-form-row')].find(x=>x.textContent.includes('${DEFAULT_STANDARD_BEHAVIOR_ID}'));const b=row&&[...row.querySelectorAll('button')].find(x=>x.textContent.trim()==='Override');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.toast')?.textContent.includes('Contribution overridden')`);
    await evalValue(cdp, sessionId, `document.querySelector('.modal-card [aria-label="Close modal"]')?.click()`);
    await waitFor(cdp, sessionId, `!document.querySelector('.modal-card')`);
    if (!await evalValue(cdp, sessionId, `(() => { const row=[...document.querySelectorAll('.data-row.bot-package-columns')].find(x=>x.textContent.includes('browser-project-package'));return Boolean(row&&[...row.querySelectorAll('button')].find(x=>x.textContent.trim()==='Override'));})()`)) fail("Project Package attached to Bot has no Bot-level Override action");

    // Simulate must exercise the actual bundled browser Engine and produce a visible chat turn.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Simulate');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.simulation-ready-panel')?.textContent.includes('Ready · Engine')`);
    await evalValue(cdp, sessionId, `(() => { const input=[...document.querySelectorAll('label')].find(x=>x.textContent.trim().startsWith('Message'))?.querySelector('input'); if(!input)return false;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'hello');input.dispatchEvent(new Event('input',{bubbles:true}));return true;})()`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='Send');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.simulation-transcript')?.textContent.includes('You') && document.querySelector('.simulation-transcript')?.textContent.includes('Bot')`);
    // Simulate sends a fixed seed, so this fixture renders exactly one canonical variant of core.smalltalk.formal.hello.response.
    if (!await evalValue(cdp, sessionId, `document.querySelector('.simulation-transcript')?.textContent.includes('Greetings.')`)) fail("Simulate did not produce the canonical deterministic chat response");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Packages');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.page-heading h1')?.textContent==='Packages'`);

    await evalValue(cdp, sessionId, `(() => { const b=document.querySelector('.bot-override-columns .row-title');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.context-tabs button')].some(x=>x.textContent.trim()==='Behaviors')`);
    const botPackageCrumb = await evalValue(cdp, sessionId, `[...document.querySelectorAll('.breadcrumb button,.breadcrumb strong')].map(x=>x.textContent.trim())`);
    for (const label of ["Projects","Browser Project","Test Bot"]) if (!botPackageCrumb.includes(label)) fail(`Bot package breadcrumb missing ${label}`);
    if (!await evalValue(cdp, sessionId, `document.querySelector('.breadcrumb-provenance')?.textContent.trim()==='Bot package'`)) fail("Bot Package breadcrumb does not expose Bot ownership");
    await evalValue(cdp, sessionId, `history.back()`);
    await waitFor(cdp, sessionId, `document.querySelector('.page-heading h1')?.textContent==='Packages' && Boolean(document.querySelector('.bot-override-columns .row-title'))`);
    await evalValue(cdp, sessionId, `history.forward()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.context-tabs button')].some(x=>x.textContent.trim()==='Behaviors')`);
    const packageTabs = await evalValue(cdp, sessionId, `[...document.querySelectorAll('.context-tabs button')].map(x=>x.textContent.trim())`);
    const expectedPackageTabs=["Overview","Behaviors","Capabilities","Source","Assets","Simulate","Audit"];
    if (JSON.stringify(packageTabs)!==JSON.stringify(expectedPackageTabs)) fail(`Package contextual tabs mismatch: ${JSON.stringify(packageTabs)}`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Overview');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.page-heading button')].some(x=>x.textContent.trim()==='Download package ZIP')`);
    if (await evalValue(cdp, sessionId, `Boolean(document.querySelector('select[aria-label="Selected package"]'))`)) fail("Package editor exposes an inline package switcher");

    // Dense authoring resources are list-first: the page stays a catalog and the full editor opens in a large modal.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Behaviors');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.object-list-page h2')?.textContent==='Behaviors'`);
    if (await evalValue(cdp, sessionId, `Boolean(document.querySelector('.split-workspace'))`)) fail("Behaviors still use a squeezed list+editor split view");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='New behavior');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `Boolean(document.querySelector('.modal-card-workspace .editor-layout'))`);
    const behaviorEditorModal = await evalValue(cdp, sessionId, `(() => { const modal=document.querySelector('.modal-card-workspace'); if(!modal)return false; const r=modal.getBoundingClientRect(); return r.width >= Math.min(1000, innerWidth-60) && r.height >= Math.min(650, innerHeight-60); })()`);
    if (!behaviorEditorModal) fail("Behavior editor is not using the large workspace modal");
    if (!await evalValue(cdp, sessionId, `Boolean([...document.querySelectorAll('.modal-card-workspace button')].find(x=>x.textContent.trim()==='Create behavior' && x.disabled))`)) fail("New Behavior draft can be committed before it has a positive sample");
    if (!await evalValue(cdp, sessionId, `document.querySelector('.modal-card-workspace')?.textContent.includes('Meaning has no positive matching evidence')`)) fail("Behavior validation is not shown inside the owning modal");
    await evalValue(cdp, sessionId, `(() => { const i=document.querySelector('.modal-card-workspace input[placeholder="What might a user say?"]');if(!i)return false;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i,'transactional hello');i.dispatchEvent(new Event('input',{bubbles:true}));return true;})()`);
    await evalValue(cdp, sessionId, `(() => { const i=document.querySelector('.modal-card-workspace input[placeholder="Response text"]');if(!i)return false;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i,'Hello from the authored response.');i.dispatchEvent(new Event('input',{bubbles:true}));return true;})()`);
    await waitFor(cdp, sessionId, `Boolean([...document.querySelectorAll('.modal-card-workspace button')].find(x=>x.textContent.trim()==='Create behavior' && !x.disabled))`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card-workspace button')].find(x=>x.textContent.trim()==='Create behavior');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `!document.querySelector('.modal-card-workspace') && document.body.textContent.includes('transactional hello')`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.behavior-list-card')].find(x=>x.textContent.includes('transactional hello'));b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `Boolean(document.querySelector('.modal-card-workspace .editor-layout'))`);
    await evalValue(cdp, sessionId, `(() => { const i=document.querySelector('.modal-card-workspace input[placeholder="What might a user say?"]');if(!i)return false;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i,'unsaved behavior edit');i.dispatchEvent(new Event('input',{bubbles:true}));return true;})()`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card-workspace button')].find(x=>x.textContent.trim()==='Cancel');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `!document.querySelector('.modal-card-workspace')`);
    if (await evalValue(cdp, sessionId, `document.body.textContent.includes('unsaved behavior edit')`)) fail("Canceling Behavior edit mutated canonical source");
    if (!await evalValue(cdp, sessionId, `document.body.textContent.includes('transactional hello')`)) fail("Saved Behavior disappeared after canceling a later edit");

    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Capabilities');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.object-list-page h2')?.textContent==='Capabilities'`);
    if (await evalValue(cdp, sessionId, `Boolean(document.querySelector('.split-workspace'))`)) fail("Capabilities still use a squeezed list+editor split view");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='New capability');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `Boolean(document.querySelector('.modal-card-workspace .editor-layout'))`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card-workspace button')].find(x=>x.textContent.trim()==='Cancel');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `!document.querySelector('.modal-card-workspace')`);
    if (await evalValue(cdp, sessionId, `document.body.textContent.includes('capability.new')`)) fail("Canceling New Capability persisted its draft");

    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Assets');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.object-list-page h2')?.textContent==='Assets'`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='Add asset');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.textContent.includes('New asset')`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card button')].find(x=>x.textContent.trim()==='Cancel');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `!document.querySelector('.modal-card')`);

    // Bot Settings are ordinary settings. Numeric differences become Bot-specific automatically; there are no redundant Override toggles.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.breadcrumb button')].find(x=>x.textContent.trim()==='Test Bot');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.context-tabs button')].some(x=>x.textContent.trim()==='Settings')`);
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.context-tabs button')].find(x=>x.textContent.trim()==='Settings');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.body.textContent.includes('Matching settings') && document.body.textContent.includes('Conversation settings')`);
    if (await evalValue(cdp, sessionId, `document.body.textContent.includes('Matching overrides') || document.body.textContent.includes('Conversation overrides') || Boolean(document.querySelector('.override-settings'))`)) fail("Bot Settings still expose override-specific UI");
    if (!await evalValue(cdp, sessionId, `[...document.querySelectorAll('input[type="number"]')].length > 0 && [...document.querySelectorAll('input[type="number"]')].every(x=>!x.disabled)`)) fail("Bot numeric settings are not directly editable");

    // Deletion always uses the same confirmation modal and overlay clicks never dismiss it.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.breadcrumb button')].find(x=>x.textContent.trim()==='Projects');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.project-row .row-title')].some(x=>x.textContent.trim()==='Browser Project')`);
    await evalValue(cdp, sessionId, `(() => { const b=document.querySelector('[aria-label="Remove Browser Project"]');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.modal-card')?.textContent.includes('Remove project?')`);
    const overlayBlocked = await evalValue(cdp, sessionId, `(() => { const bg=document.querySelector('.modal-backdrop');bg?.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));return Boolean(document.querySelector('.modal-card'));})()`);
    if (!overlayBlocked) fail("confirmation modal dismissed from overlay click");
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.modal-card button')].find(x=>x.textContent.trim()==='Cancel');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `!document.querySelector('.modal-card')`);

    // Global Settings exist separately from Bot Settings.
    await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.nav-button')].find(x=>x.textContent.trim()==='Settings');b?.click();return Boolean(b);})()`);
    await waitFor(cdp, sessionId, `document.querySelector('.page-heading h1')?.textContent==='Settings' && document.body.textContent.includes('Matching defaults')`);

    // Filesystem content autosave must preserve the current Project across reload. Wait for the
    // autosave write to actually land instead of racing Studio's debounce with a fixed sleep.
    await waitForAutosave(server, contentRoot);
    await cdp.send("Page.reload", { ignoreCache: true }, sessionId);
    await waitFor(cdp, sessionId, `document.readyState === "complete" && Boolean(document.querySelector(".app-shell"))`);
    await waitFor(cdp, sessionId, `Boolean([...document.querySelectorAll('.nav-button')].find(x=>x.textContent.trim()==='Projects') || document.querySelector('.error-banner'))`);
    const reloadError = await evalValue(cdp, sessionId, `document.querySelector('.error-banner')?.textContent ?? ''`);
    if (reloadError) fail(`filesystem content reload failed: ${reloadError}`);
    if (!await evalValue(cdp, sessionId, `(() => { const b=[...document.querySelectorAll('.nav-button')].find(x=>x.textContent.trim()==='Projects');b?.click();return Boolean(b);})()`)) fail("Projects navigation was unavailable after filesystem content reload");
    await waitFor(cdp, sessionId, `[...document.querySelectorAll('.project-row .row-title')].some(x=>x.textContent.trim()==='Browser Project')`);
    if (runtimeErrors.length) fail(`browser runtime exceptions: ${runtimeErrors.join(" | ")}`);
    if (consoleErrors.length) fail(`browser console/log errors: ${consoleErrors.join(" | ")}`);

    console.log("PASS minimal global navigation + breadcrumb hierarchy");
    console.log("PASS row-based Project/Bot lists");
    console.log("PASS unified blocking draggable system modals");
    console.log("PASS Bot Overview/Packages separation + Bot-only Override flow + Package editor navigation");
    console.log("PASS provider-free Studio navigation and persistence");
    console.log("PASS global defaults + simple Bot settings without override toggles");
    console.log("PASS portable filesystem content autosave persistence");
    console.log("PASS no runtime exceptions / error-level browser log entries");
    console.log(JSON.stringify({ passed: 8, failed: 0 }));
  } finally {
    try { cdp?.close(); } catch {}
    if (browser.exitCode === null) {
      browser.kill("SIGTERM");
      await Promise.race([
        new Promise((resolvePromise) => browser.once("exit", resolvePromise)),
        sleep(2000),
      ]);
      if (browser.exitCode === null) {
        browser.kill("SIGKILL");
        await Promise.race([
          new Promise((resolvePromise) => browser.once("exit", resolvePromise)),
          sleep(2000),
        ]);
      }
    }
    server?.close();
    await rm(temporaryRoot, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
    if (browser.exitCode && browser.exitCode !== 0 && browserStderr) process.stderr.write(browserStderr);
  }
}

main().catch((error) => { console.error(`BROWSER ACCEPTANCE FAIL: ${error.stack ?? error}`); process.exit(1); });
