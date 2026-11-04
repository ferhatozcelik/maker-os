// makerd API v1 istemcisi (docs/v2/api.md). fetch sarmalayıcı + events WS.
import { Emitter } from "./lib.js";

export const BASE = "/api/v1";
export const bus = new Emitter(); // "unauthorized", "offline", "online"

export class ApiError extends Error {
  constructor(status, code, message) { super(message); this.status = status; this.code = code; }
}

export async function api(method, path, body, { timeout = 15000, keepalive = false } = {}) {
  const ctl = new AbortController(), timer = setTimeout(() => ctl.abort(), timeout);
  const headers = { Accept: "application/json" };
  if (method !== "GET") headers["X-Maker"] = "1";
  if (body !== undefined) headers["Content-Type"] = "application/json";
  let res;
  try {
    res = await fetch(BASE + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), credentials: "same-origin", signal: ctl.signal, keepalive });
  } catch (e) {
    bus.emit("offline");
    throw new ApiError(0, "unavailable", e.name === "AbortError" ? "Zaman aşımı" : "makerd'e ulaşılamıyor");
  } finally { clearTimeout(timer); }
  if (res.status === 204 || res.status === 302) return null;
  let data = null;
  try { data = await res.json(); } catch { /* gövde yok */ }
  if (!res.ok) {
    const e = data?.error || {};
    if (res.status === 401 && path !== "/auth/login") bus.emit("unauthorized");
    throw new ApiError(res.status, e.code || "internal", e.message || `HTTP ${res.status}`);
  }
  return data;
}
export const get = p => api("GET", p);
export const post = (p, b = {}) => api("POST", p, b);
export const put = (p, b) => api("PUT", p, b);

export const wsURL = p => `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${BASE}${p}`;

// Olay akışı: konu aboneliği, otomatik yeniden bağlanma (üstel geri çekilme)
export class Events extends Emitter {
  #ws = null; #subs = new Map(); #retry = 0; #timer = 0; #want = false;
  start() { this.#want = true; this.#connect(); return this; }
  stop() { this.#want = false; clearTimeout(this.#timer); this.#ws?.close(); this.#ws = null; }
  get connected() { return this.#ws?.readyState === 1; }
  #connect() {
    if (!this.#want || this.#ws) return;
    const ws = this.#ws = new WebSocket(wsURL("/events"));
    ws.onopen = () => {
      this.#retry = 0; this.emit("open");
      const topics = [...this.#subs.keys()];
      if (topics.length) ws.send(JSON.stringify({ op: "sub", topics }));
    };
    ws.onmessage = e => {
      let m; try { m = JSON.parse(e.data); } catch { return; }
      for (const f of this.#subs.get(m.topic) || []) try { f(m.data); } catch (x) { console.error(x); }
    };
    ws.onclose = () => {
      if (this.#ws !== ws) return;
      this.#ws = null; this.emit("close");
      if (!this.#want) return;
      const d = Math.min(15000, 500 * 2 ** this.#retry++);
      this.#timer = setTimeout(() => this.#connect(), d);
    };
  }
  // fn(data) çağrılır; dönen işlev aboneliği kaldırır
  subscribe(topic, fn) {
    let set = this.#subs.get(topic);
    if (!set) {
      this.#subs.set(topic, set = new Set());
      if (this.connected) this.#ws.send(JSON.stringify({ op: "sub", topics: [topic] }));
    }
    set.add(fn);
    return () => {
      set.delete(fn);
      if (!set.size) {
        this.#subs.delete(topic);
        if (this.connected) this.#ws.send(JSON.stringify({ op: "unsub", topics: [topic] }));
      }
    };
  }
}
