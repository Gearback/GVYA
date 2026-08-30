import { GvyaRuntime, WasmRuntimeBackend, unsignedDevelopmentOpenOptions } from "./sdk/index.js";

const status = document.querySelector("#runtime-status");
const stateBadge = status.parentElement;
const launcher = document.querySelector("#chat-launcher");
const panel = document.querySelector("#chat-panel");
const closeButton = document.querySelector("#chat-close");
const scrim = document.querySelector("#scrim");
const form = document.querySelector("#chat-form");
const input = document.querySelector("#chat-input");
const sendButton = document.querySelector("#send-button");
const messages = document.querySelector("#messages");
const suggestions = document.querySelector("#suggestions");

let runtime;
let conversationState;
let busy = false;
let openingShown = false;

bootstrap().catch((error) => {
  console.error(error);
  status.textContent = "Runtime failed to load";
  stateBadge.classList.add("error");
  launcher.disabled = true;
});

async function bootstrap() {
  const [wasmResponse, artifactResponse] = await Promise.all([
    fetch("./gvya-ffi-v1.wasm"),
    fetch("./gvya-bot.gvya"),
  ]);
  requireResponse(wasmResponse);
  requireResponse(artifactResponse);
  const [wasmBytes, artifactBytes] = await Promise.all([
    wasmResponse.arrayBuffer(),
    artifactResponse.arrayBuffer(),
  ]);
  const backend = await WasmRuntimeBackend.instantiate(wasmBytes);
  runtime = await GvyaRuntime.open(new Uint8Array(artifactBytes), backend, unsignedDevelopmentOpenOptions());
  const info = await runtime.info();
  status.textContent = `Runtime ready · ${info.enabled_languages.length} sample languages loaded`;
  stateBadge.classList.add("ready");
  launcher.disabled = false;
}

function requireResponse(response) {
  if (!response.ok) throw new Error(`HTTP ${response.status} for ${response.url}`);
  return response;
}

async function openChat() {
  if (launcher.disabled) return;
  panel.classList.add("open");
  scrim.classList.add("open");
  panel.setAttribute("aria-hidden", "false");
  launcher.setAttribute("aria-expanded", "true");
  if (!openingShown) await showOpening();
  window.setTimeout(() => input.focus(), 50);
}

async function showOpening() {
  if (!runtime || openingShown) return;
  openingShown = true;
  setBusy(true);
  const typing = appendTyping();
  try {
    const result = await runtime.openConversation({
      format: "gvya.runtime.open",
      version: 1,
      context: { values: {}, available_capabilities: [], visible_references: [] },
      seed: null,
    });
    conversationState = result.state;
    typing.remove();
    const replies = extractTextItems(result.response);
    if (replies.length === 0) {
      appendBubble("bot", "Hi — I’m GVYA. Ask me what GVYA is or what you can build with it.", "ltr");
    } else {
      replies.forEach((reply, index) => appendBubble("bot", reply.text, directionFor(reply.text, reply.language), index > 0));
    }
  } catch (error) {
    console.error(error);
    typing.remove();
    appendBubble("bot", "Hi — I’m GVYA. Ask me what GVYA is or what you can build with it.", "ltr");
  } finally {
    setBusy(false);
  }
}

function closeChat() {
  panel.classList.remove("open");
  scrim.classList.remove("open");
  panel.setAttribute("aria-hidden", "true");
  launcher.setAttribute("aria-expanded", "false");
  launcher.focus();
}

launcher.addEventListener("click", openChat);
closeButton.addEventListener("click", closeChat);
scrim.addEventListener("click", closeChat);
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && panel.classList.contains("open")) closeChat();
});

suggestions.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-suggestion]");
  if (!button || busy) return;
  input.value = button.dataset.suggestion ?? "";
  form.requestSubmit();
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const text = input.value.trim();
  if (!text || busy || !runtime) return;
  appendBubble("user", text, directionFor(text));
  input.value = "";
  setBusy(true);
  const typing = appendTyping();
  try {
    const result = await runtime.turn({
      format: "gvya.runtime.turn",
      version: 1,
      utterance: { text },
      context: { values: {}, available_capabilities: [], visible_references: [] },
      ...(conversationState === undefined ? {} : { state: conversationState }),
      seed: null,
    });
    conversationState = result.state;
    typing.remove();
    const replies = extractTextItems(result.response);
    if (replies.length === 0) {
      appendBubble("bot", "GVYA returned no text response for this turn.", "ltr");
    } else {
      replies.forEach((reply, index) => appendBubble("bot", reply.text, directionFor(reply.text, reply.language), index > 0));
    }
  } catch (error) {
    console.error(error);
    typing.remove();
    appendBubble("bot", "The local GVYA runtime could not complete that turn.", "ltr");
  } finally {
    setBusy(false);
    input.focus();
  }
});

function extractTextItems(response) {
  const output = [];
  if (!response || typeof response !== "object" || !Array.isArray(response.messages)) return output;
  for (const message of response.messages) {
    if (!message || !Array.isArray(message.items)) continue;
    for (const item of message.items) {
      if (item?.type === "text" && typeof item.text === "string" && item.text.trim()) {
        output.push({ text: item.text, language: typeof item.language === "string" ? item.language : "" });
      }
    }
  }
  return output;
}

function appendBubble(role, text, dir, continuation = false) {
  const row = document.createElement("div");
  row.className = `message-row ${role}${continuation ? " continuation" : ""}`;
  const bubble = document.createElement("div");
  bubble.className = "bubble";
  bubble.dir = dir;
  bubble.textContent = text;
  row.append(bubble);
  messages.append(row);
  messages.scrollTop = messages.scrollHeight;
  return row;
}

function appendTyping() {
  const row = document.createElement("div");
  row.className = "message-row bot";
  row.setAttribute("aria-label", "GVYA is responding");
  row.innerHTML = '<div class="bubble typing"><i></i><i></i><i></i></div>';
  messages.append(row);
  messages.scrollTop = messages.scrollHeight;
  return row;
}

function directionFor(text, language = "") {
  if (/^(fa|ar|ur)(-|$)/i.test(language)) return "rtl";
  return /[\u0600-\u06FF\u0750-\u077F\u08A0-\u08FF]/u.test(text) ? "rtl" : "ltr";
}

function setBusy(value) {
  busy = value;
  input.disabled = value;
  sendButton.disabled = value;
  suggestions.querySelectorAll("button").forEach((button) => { button.disabled = value; });
}

window.addEventListener("pagehide", () => { runtime?.close().catch(() => {}); });
