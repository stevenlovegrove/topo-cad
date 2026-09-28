// The engine, off the UI thread: the topo-cad wasm module (model building,
// analysis, drawings, 3D) plus the browser's own JavaScript engine running
// the model scripts. Messages: {id, op, args} → {id, ok, value | error}.
import init, * as wasm from "../pkg/topo_wasm.js";

const ready = init().then(() => {
  // Native helpers the topo-cad API calls from scripts.
  globalThis.__topo_data = (name) => wasm.table_json(name);
  globalThis.__topo_truss_standard = (req) => wasm.standard_truss_json(req);
});

let app = null;
// One version counter across projects, so the UI sees every rebuild as new.
let generation = 0;
const rebuilt = () => {
  app.rebuild();
  return ++generation;
};

const ops = {
  open({ entry, files }) {
    app?.free();
    app = new wasm.App(entry, JSON.stringify(files));
    return rebuilt();
  },
  setFiles({ files }) {
    app.set_files(JSON.stringify(files));
    return rebuilt();
  },
  scenario({ name }) {
    app.set_scenario(name || "");
    return rebuilt();
  },
  exclude({ names }) {
    app.set_excluded(JSON.stringify(names));
    return rebuilt();
  },
  state() {
    return app ? { ...JSON.parse(app.state()), version: generation } : null;
  },
  request({ op, args }) {
    const out = app.request(op, JSON.stringify(args ?? {}));
    return op === "sheet" ? out : JSON.parse(out);
  },
  measure(args) {
    return JSON.parse(app.measure(JSON.stringify(args)));
  },
  imports({ source, id }) {
    return JSON.parse(wasm.module_imports(source, id));
  },
  sidecarId({ id }) {
    return wasm.sidecar_id(id);
  },
  newProject({ name }) {
    return JSON.parse(wasm.new_project(name));
  },
};

self.onmessage = async (e) => {
  const { id, op, args } = e.data;
  try {
    await ready;
    if (!ops[op]) throw new Error(`unknown engine op ${op}`);
    if (!app && !["open", "state", "imports", "sidecarId", "newProject"].includes(op)) throw new Error("no model open");
    self.postMessage({ id, ok: true, value: ops[op](args ?? {}) });
  } catch (err) {
    self.postMessage({ id, ok: false, error: String(err?.message ?? err) });
  }
};
