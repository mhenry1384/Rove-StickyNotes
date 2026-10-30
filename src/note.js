import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

// The note grows and shrinks to fit its lines, up to this many; after that the text scrolls.
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
  pin.title = onTop ? "Pinned on top (click to unpin)" : "Pin on top";
}

// Size the window to fit the text (up to MAX_AUTO_LINES lines).
async function fitToText() {
  measure.style.width = `${text.clientWidth}px`;
  measure.value = text.value;
  const contentHeight = measure.scrollHeight;
  const maxHeight = MAX_AUTO_LINES * lineHeight + 2 * padY;
  const textHeight = Math.min(contentHeight, maxHeight);
  // Size the textarea right away so it never has to scroll while the window catches up;
  // otherwise the text jumps up past the top margin for a frame on each new line.
  text.style.height = `${textHeight}px`;
  const wanted = Math.ceil(titlebarHeight + textHeight + BORDER);
  if (Math.abs(window.innerHeight - wanted) >= 1) {
    await invoke("fit_note_height", { height: wanted });
  }
}

text.addEventListener("input", () => {
  invoke("update_note_text", { id, text: text.value });
  fitToText();
});

// Drag the left or right edge to change the width.
for (const edge of document.querySelectorAll(".edge")) {
  edge.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    edge.setPointerCapture(e.pointerId);
    const startX = e.screenX;
    const startWidth = window.innerWidth;
    const right = edge.id === "edge-left" ? window.screenX + startWidth : null;
    const onMove = (e) => {
      const dx = e.screenX - startX;
      const width = right === null ? startWidth + dx : startWidth - dx;
      invoke("resize_note_width", { width, right });
    };
    const onUp = () => {
      edge.removeEventListener("pointermove", onMove);
      edge.removeEventListener("lostpointercapture", onUp);
    };
    edge.addEventListener("pointermove", onMove);
    edge.addEventListener("lostpointercapture", onUp);
  });
}

// Belt and braces for overflow: clip in note.css; the page itself should never scroll.
window.addEventListener("scroll", () => window.scrollTo(0, 0));

// Changing the width re-wraps the text, so the number of lines may change.
window.addEventListener("resize", fitToText);

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
