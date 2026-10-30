import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

// The note grows as lines are added, up to this many; after that the text scrolls.
const MAX_AUTO_LINES = 10;
// Top + bottom window border (see body in note.css).
const BORDER = 2;

const appWindow = getCurrentWindow();
const id = appWindow.label.replace(/^note-/, "");

const titlebar = document.getElementById("titlebar");
const pin = document.getElementById("pin");
const closeButton = document.getElementById("close");
const text = document.getElementById("text");
const measure = document.getElementById("measure");

const css = getComputedStyle(document.documentElement);
const px = (name) => parseFloat(css.getPropertyValue(name));
const lineHeight = px("--line-height");
const titlebarHeight = px("--titlebar-height");
const padY = px("--pad-y");

let onTop = false;

function setPin(value) {
  onTop = value;
  pin.classList.toggle("on", onTop);
  pin.title = onTop ? "Stop keeping on top" : "Keep on top";
}

// Grow the window so all the text is visible (up to MAX_AUTO_LINES lines).
async function fitToText() {
  measure.style.width = `${text.clientWidth}px`;
  measure.value = text.value;
  const contentHeight = measure.scrollHeight;
  const maxHeight = MAX_AUTO_LINES * lineHeight + 2 * padY;
  const wanted = Math.ceil(titlebarHeight + Math.min(contentHeight, maxHeight) + BORDER);
  if (window.innerHeight < wanted) {
    await invoke("grow_note", { height: wanted });
  }
}

text.addEventListener("input", () => {
  invoke("update_note_text", { id, text: text.value });
  fitToText();
});

titlebar.addEventListener("mousedown", (e) => {
  if (e.button === 0 && !e.target.closest("button")) {
    appWindow.startDragging();
  }
});

pin.addEventListener("click", async () => {
  setPin(!onTop);
  await invoke("set_on_top", { onTop });
  text.focus();
});

closeButton.addEventListener("click", async () => {
  await invoke("update_note_text", { id, text: text.value });
  await appWindow.close();
});

// Suppress the browser context menu everywhere except the text area (which keeps cut/copy/paste).
document.addEventListener("contextmenu", (e) => {
  if (e.target !== text) e.preventDefault();
});

invoke("get_note", { id }).then((note) => {
  if (note) {
    text.value = note.text;
    setPin(note.on_top);
  }
  text.focus();
  text.setSelectionRange(text.value.length, text.value.length);
  fitToText();
});
