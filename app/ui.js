// Small things every part of the page uses: finding and making elements, text made safe to put
// in them, what is kept in this browser between visits, a menu at the cursor.

export const $ = (id) => document.getElementById(id);
export const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
/** An element: its attributes (`class` among them), and what is in it as HTML. */
export const el = (tag, attrs = {}, html) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) if (k === "class") e.className = v; else e.setAttribute(k, v);
  if (html != null) e.innerHTML = html;
  return e;
};

/** What this browser has kept under a key (as JSON); `fallback` where it has nothing, or cannot say. */
export function stored(key, fallback) {
  try { return JSON.parse(localStorage.getItem(key) ?? "null") ?? fallback; } catch (_) { return fallback; }
}
/** Keeps a value under a key in this browser (where it lets us). */
export function store(key, value) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch (_) {}
}

/** Waits (a few seconds at most) for `f()` to be something, looking again every so often; what it is then. */
export async function until(f, ms = 2500) {
  for (const t0 = performance.now(); performance.now() - t0 < ms; ) { const x = f(); if (x) return x; await new Promise((r) => setTimeout(r, 50)); }
  return f();
}

/**
 * A menu at the cursor: [{ label, run, cls }], null for a divider; one with `items` in place of `run` opens
 * a menu of those beside it (when the cursor comes onto it, or it is clicked). It closes on a choice, a click
 * elsewhere, or Esc.
 */
export function openMenu(items, x, y) {
  closeMenu();
  const made = (items, root) => {
    const menu = el("div", { class: "ctxmenu", ...(root ? { id: "ctxmenu" } : {}) });
    // (One of its items' menus is open at a time.)
    const shut = () => { menu.sub?.shut(); menu.sub?.el.remove(); menu.sub = null; for (const b of menu.children) b.classList.remove("open"); };
    for (const it of items) {
      if (!it) { menu.appendChild(el("hr")); continue; }
      const b = el(it.cls === "head" ? "div" : "button", { class: `${it.cls || ""}${it.items ? " more" : ""}` });
      // (An item that is on or off: a box before it, ticked where it is on.)
      b.textContent = it.checked === undefined ? it.label : `${it.checked ? "☑" : "☐"} ${it.label}`;
      if (it.title) b.title = it.title;
      if (it.items) {
        const open = () => {
          if (menu.sub?.of === b) return;
          shut();
          const sub = made(it.items, false), r = b.getBoundingClientRect();
          document.body.appendChild(sub.el);
          // Beside it, on whichever side there is room; inside the window.
          const w = sub.el.offsetWidth, h = sub.el.offsetHeight, right = r.right - 3 + w <= window.innerWidth - 6;
          sub.el.style.left = `${Math.max(4, right ? r.right - 3 : r.left + 3 - w)}px`;
          sub.el.style.top = `${Math.max(4, Math.min(r.top - 4, window.innerHeight - h - 6))}px`;
          menu.sub = { ...sub, of: b };
          b.classList.add("open");
        };
        b.onpointerenter = open;
        b.onclick = open;
      } else {
        // (Onto a plain item: no other's menu stays open beside it.)
        if (it.cls !== "head") b.onpointerenter = shut;
        b.onclick = () => { closeMenu(); it.run(); };
      }
      menu.appendChild(b);
    }
    return { el: menu, shut };
  };
  const menu = made(items, true).el;
  document.body.appendChild(menu);
  // Inside the window.
  menu.style.left = `${Math.max(4, Math.min(x, window.innerWidth - menu.offsetWidth - 6))}px`;
  menu.style.top = `${Math.max(4, Math.min(y, window.innerHeight - menu.offsetHeight - 6))}px`;
}
function closeMenu() {
  for (const m of document.querySelectorAll(".ctxmenu")) m.remove();
}
document.addEventListener("pointerdown", (e) => { if (!e.target.closest?.(".ctxmenu")) closeMenu(); }, true);
document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeMenu(); });

/** Says something beside the cursor (`e`: where it is; none: no longer). */
export function tip(text, e) {
  const t = $("tip");
  t.style.display = text ? "block" : "none";
  if (!text) return;
  t.style.whiteSpace = text.includes("\n") ? "pre" : "nowrap";
  t.textContent = text;
  // Kept on the page.
  const w = t.offsetWidth, h = t.offsetHeight;
  t.style.left = `${Math.max(4, Math.min(e.clientX + 16, window.innerWidth - w - 6))}px`;
  t.style.top = `${e.clientY + 16 + h > window.innerHeight - 6 ? Math.max(4, e.clientY - h - 12) : e.clientY + 16}px`;
}
