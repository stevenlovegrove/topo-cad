// A project's model as last settled here, kept in this browser (IndexedDB): the settled model as data (the engine's
// `settledOut`: bytes), with what it was settled from (`keyOf`: which build of the engine, the model, its scenario,
// the load cases left out, and its files). A page opening a project whose model was settled from just that shows it
// from what is kept, at once, while the engine builds it again behind it. One kept a project: the last.

const DB = "topo-cad-settled", STORE = "models";
let db = null;
const opened = () => (db ??= new Promise((done, fail) => {
  const r = indexedDB.open(DB, 1);
  r.onupgradeneeded = () => r.result.createObjectStore(STORE);
  r.onsuccess = () => done(r.result);
  r.onerror = () => fail(r.error);
}));
const ask = async (mode, f) => {
  const d = await opened();
  return new Promise((done, fail) => {
    const t = d.transaction(STORE, mode), r = f(t.objectStore(STORE));
    t.oncomplete = () => done(r?.result);
    t.onerror = t.onabort = () => fail(t.error);
  });
};

/** What a model settled from these is kept by: its engine's build (`stamp`), the model (`entry`, in the store that
 *  keeps it: `place`), its scenario and the load cases left out, and each of its files (not what is kept beside them
 *  of how they were settled, or drawn: what is settled is of the files alone). */
export async function keyOf({ stamp, place, entry, files, scenario, names }) {
  const own = Object.keys(files).filter((id) => !/\.(solved|drawn)\.json$/.test(id)).sort();
  const text = JSON.stringify([stamp, place, entry, scenario ?? null, names ?? null, own.map((id) => [id, files[id]])]);
  const hash = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(hash)].map((b) => b.toString(16).padStart(2, "0")).join("");
}
/** The model of `id` as kept here, where it was settled from what `key` says: its bytes; else null. */
export async function kept(id, key) {
  try {
    const v = await ask("readonly", (s) => s.get(id));
    return v?.key === key ? v.bytes : null;
  } catch (_) {
    return null;
  }
}
/** Keeps the model of `id` as settled from what `key` says (in place of what was kept of it). */
export async function keep(id, key, bytes) {
  try {
    await ask("readwrite", (s) => s.put({ key, bytes }, id));
  } catch (_) {
    // (A browser that keeps nothing: the next page builds it.)
  }
}
