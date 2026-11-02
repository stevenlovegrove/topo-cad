// A facet session, off the page's thread: the topo-session wasm module, reading its files through byte ranges (as a worker
// can, at once: a request that waits, or a file's slice). Messages in: { id, call, ... }; out: { id, result } or { id, error }.
//   open { recording, layer, calibration, trajectory }   each file as the store gives it ({ url } or { file }; a layer, what
//                                                        may not be there). Its cameras, times and path.
//   view { times: [[camera, sequence]...] }              each frame as its camera saw it
//   at { t }                                             where each camera was at a time
//   jpeg { camera, sequence }                            a frame's JPEG, its bytes
//   preview { camera, sequence, width }                  a frame made small, an ImageBitmap (a frame while seeking)
//   slate { camera, sequence, width }                    a frame made small, its pixels: { w, h, rgba } (RGBA bytes)
// And, as it goes, { progress: { what, share } }: what it is reading, and how far through (where that can be said).
import init, { Frames } from "../pkg/topo_session.js";

const ready = init({ module_or_path: new URL("../pkg/topo_session_bg.wasm", import.meta.url) });

/** `length` bytes from `offset` of a file: a `Uint8Array`. */
function read(source, offset, length) {
  if (source.file) return new Uint8Array(new (/** @type {any} */ (globalThis).FileReaderSync)().readAsArrayBuffer(source.file.slice(offset, offset + length)));
  const r = new XMLHttpRequest();
  r.open("GET", source.url, false);
  r.responseType = "arraybuffer";
  r.setRequestHeader("Range", `bytes=${offset}-${offset + length - 1}`);
  r.send();
  if (r.status !== 206 && r.status !== 200) throw new Error(`${source.url}: ${r.status}`);
  return new Uint8Array(r.response);
}
/** How long a file is; null where it is not there. */
function sizeOf(source) {
  if (!source) return null;
  if (source.file) return source.file.size;
  const r = new XMLHttpRequest();
  r.open("HEAD", source.url, false);
  r.send();
  return r.status === 200 ? Number(r.getResponseHeader("Content-Length")) : null;
}
const whole = (source) => {
  const n = sizeOf(source);
  if (n == null) throw new Error(`${source?.url ?? "a session's file"} is not there`);
  return read(source, 0, n);
};

let frames = null, recording = null, said = 0;
const progress = (what, share = null) => self.postMessage({ progress: { what, share } });

const calls = {
  open(m) {
    recording = m.recording;
    progress("its trajectory");
    const calibration = whole(m.calibration), trajectory = whole(m.trajectory), layer = sizeOf(m.layer);
    if (layer != null) {
      progress("its frames");
      frames = Frames.of_layer(read(m.layer, 0, layer), calibration, trajectory);
    } else {
      const size = sizeOf(recording);
      if (size == null) throw new Error(`${recording.url ?? "its recording"} is not there`);
      // (Through the file in order, past its other topics: how far is how much is done. At most a few times a second.)
      const reading = (offset, length) => {
        const now = performance.now();
        if (now - said > 150 && offset < size - (1 << 20)) { said = now; progress("its frames", offset / size); }
        return read(recording, offset, length);
      };
      progress("its frames", 0);
      frames = Frames.of_recording(reading, size, calibration, trajectory);
    }
    const cameras = JSON.parse(frames.cameras());
    return { cameras, interval: [...frames.interval()], times: cameras.map((_, k) => frames.times(k)), sequences: cameras.map((_, k) => frames.sequences(k)), path: frames.path(Math.min(2000, Math.ceil((frames.interval()[1] - frames.interval()[0]) * 10) + 2)) };
  },
  view: (m) => m.frames.map(([camera, sequence]) => JSON.parse(frames.seen(camera, sequence))),
  at: (m) => JSON.parse(frames.cameras()).map((_, k) => [...frames.camera_at(k, m.t)]),
  jpeg(m) {
    const [offset, length] = frames.jpeg_at(m.camera, m.sequence);
    return read(recording, offset, length);
  },
  async preview(m) {
    const [offset, length] = frames.jpeg_at(m.camera, m.sequence);
    return createImageBitmap(new Blob([read(recording, offset, length)], { type: "image/jpeg" }), { resizeWidth: m.width, resizeQuality: "low" });
  },
  async slate(m) {
    const [offset, length] = frames.jpeg_at(m.camera, m.sequence);
    const b = await createImageBitmap(new Blob([read(recording, offset, length)], { type: "image/jpeg" }), { resizeWidth: m.width, resizeQuality: "medium" });
    const c = new OffscreenCanvas(b.width, b.height).getContext("2d");
    c.drawImage(b, 0, 0);
    const out = { w: b.width, h: b.height, rgba: new Uint8Array(c.getImageData(0, 0, b.width, b.height).data.buffer) };
    b.close();
    return out;
  },
};

self.onmessage = async (e) => {
  const { id, call } = e.data;
  try {
    await ready;
    const result = await calls[call](e.data);
    self.postMessage({ id, result }, result instanceof ImageBitmap ? [result] : result?.rgba ? [result.rgba.buffer] : []);
  } catch (err) {
    self.postMessage({ id, error: String(err?.message ?? err) });
  }
};
