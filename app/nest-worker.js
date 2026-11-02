// A nesting search, off the page's thread: the topo-nest wasm module (sparrow) given a job, saying each layout
// as it is come to. Messages in: {job} (topo_nest::Job). Out: {report} for each layout said (topo_nest::Report:
// one being tried, or the shortest yet), then {done} with the best, or {error}. It runs to its time and is not
// told anything meanwhile: to stop it, the page lets go of the worker (the best is already the page's).
import init, { nest } from "../pkg/topo_nest.js";

const ready = init({ module_or_path: new URL("../pkg/topo_nest_bg.wasm", import.meta.url) });

self.onmessage = async (e) => {
  try {
    await ready;
    const best = nest(JSON.stringify(e.data.job), (r) => self.postMessage({ report: JSON.parse(r) }));
    self.postMessage({ done: JSON.parse(best) });
  } catch (err) {
    self.postMessage({ error: String(err?.message ?? err) });
  }
};
