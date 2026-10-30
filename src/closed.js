import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const list = document.getElementById("list");
const empty = document.getElementById("empty");
const preview = document.getElementById("preview");
const previewText = document.getElementById("preview-text");
const hint = document.getElementById("hint");
const closedAt = document.getElementById("closed-at");
const restoreButton = document.getElementById("restore");

let notes = [];
let selectedId = null;

function formatDate(ms) {
  return ms ? new Date(ms).toLocaleString() : "";
}

function firstLine(text) {
  return text.split("\n").find((line) => line.trim()) ?? "";
}

function select(id) {
  selectedId = id;
  const note = notes.find((n) => n.id === id);
  for (const li of list.children) {
    li.classList.toggle("selected", li.dataset.id === id);
  }
  preview.hidden = !note;
  hint.hidden = !!note || notes.length === 0;
  restoreButton.disabled = !note;
  previewText.textContent = note ? note.text : "";
  closedAt.textContent = note ? `Closed ${formatDate(note.closed_at)}` : "";
}

async function refresh() {
  notes = await invoke("get_closed");
  list.replaceChildren(
    ...notes.map((note) => {
      const li = document.createElement("li");
      li.dataset.id = note.id;
      const title = document.createElement("div");
      title.className = "title";
      title.textContent = firstLine(note.text);
      const date = document.createElement("div");
      date.className = "date";
      date.textContent = formatDate(note.closed_at);
      li.append(title, date);
      li.addEventListener("click", () => select(note.id));
      return li;
    })
  );
  empty.hidden = notes.length > 0;
  select(notes.some((n) => n.id === selectedId) ? selectedId : null);
}

restoreButton.addEventListener("click", async () => {
  if (!selectedId) return;
  restoreButton.disabled = true;
  const id = selectedId;
  selectedId = null;
  await invoke("restore_note", { id });
});

listen("closed-changed", refresh);
refresh();
