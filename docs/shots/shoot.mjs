// Take the website's screenshots from the running app.
//   node shoot.mjs <port> <out dir> [skin ...]

import { writeFileSync } from "node:fs";
import { join } from "node:path";

const [port, out, ...only] = process.argv.slice(2);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));

let next = 0;
const pending = new Map();
const problems = [];
ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id !== undefined && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  } else if (msg.method === "Runtime.exceptionThrown") {
    problems.push("EXCEPTION " + JSON.stringify(msg.params.exceptionDetails).slice(0, 300));
  }
});
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++next;
    pending.set(id, (m) => (m.error ? reject(new Error(m.error.message)) : resolve(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails).slice(0, 300));
  return r.result.value;
};
const invoke = (cmd, args) =>
  evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args ?? {})})`);
const shot = async (name) => {
  const { data } = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  writeFileSync(join(out, name + ".png"), Buffer.from(data, "base64"));
  console.log("   saved", name);
};
const key = async (k) => {
  const vk = { ArrowRight: 39, ArrowLeft: 37, ArrowDown: 40, ArrowUp: 38 }[k];
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: k, code: k, windowsVirtualKeyCode: vk });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code: k, windowsVirtualKeyCode: vk });
};
// Every image on the page has arrived and been drawn.
const settle = async () => {
  await evaluate(`Promise.all(Array.from(document.images).map((i) => i.complete ? 0 : new Promise((r) => { i.onload = i.onerror = r; })))`);
  await sleep(1500);
};

await send("Runtime.enable");
await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 856, deviceScaleFactor: 2, mobile: false });

const SKINS = [
  // skin, file, how many steps right to the game it should show
  ["launchbox", "skin-arcadex", 0],
  ["switch", "skin-console", 3],
  ["steam", "skin-lounge", 4],
  ["arc", "skin-arc", 0],
  ["browser", "skin-browser", 0],
  ["wii", "skin-channels", 0],
];

for (const [skin, file, steps] of SKINS) {
  if (only.length && !only.includes(skin)) continue;
  console.log(`\n== ${skin}`);
  problems.length = 0;
  await invoke("save_settings", { values: { ui_skin: skin } });
  await send("Page.reload", { ignoreCache: true });
  await sleep(2500);
  if (skin === "launchbox") {
    // The desktop layout with a game's panel open, which is what it is for.
    await evaluate(`(() => { const t = Array.from(document.querySelectorAll("[data-game-id]")).find((e) => e.textContent.includes("Super Mario 64")); t?.click(); })()`);
  } else {
    for (let i = 0; i < steps; i++) {
      await key("ArrowRight");
      await sleep(250);
    }
  }
  await settle();
  // The desktop panel's close button takes focus when it opens; a ring on
  // it is noise in a still picture.
  if (skin === "launchbox") await evaluate(`document.activeElement?.blur()`);
  await sleep(300);
  await shot(file);
  if (problems.length) console.log(problems.join("\n"));
}

ws.close();
