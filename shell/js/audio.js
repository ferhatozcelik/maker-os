// Tarayıcı hoparlörü ve mikrofonu: Pi'de çalışan programların (pygame, PySide6, Python, Chromium) sesi
// bu sekmeden çalar; mikrofon açıkken bu bilgisayarın mikrofonu programlara gider.
// Akış: WS /api/v1/audio (bkz. makerd/src/audio/stream.rs)
//   → {"t":"fmt","rate","ch","in_rate","in_ch"}, ikili: s16le araya geçmeli PCM
//   ← {"t":"spk","on"}, {"t":"mic","on"}, ikili: s16le mono PCM (mikrofon)
// "Hoparlör izni": tarayıcılar sesi ancak kullanıcı sayfaya bir kez dokunduktan sonra çalar (otomatik
// oynatma kuralı); hoparlör açık ama bekliyorsa ilk tıklamada başlar. Mikrofon izni tarayıcının kendi
// penceresiyle sorulur ve yalnız güvenli sayfada (HTTPS ya da localhost) mümkündür.

const LS = "mk.audio.spk";
const LEAD = 0.06, MAX_LAG = 0.35; // oynatma tamponu: 60 ms hedef, 350 ms üstü atlanır

const WORKLET = `class P extends AudioWorkletProcessor{process(i){const c=i[0];if(c&&c[0])this.port.postMessage(c[0].slice(0));return true}}registerProcessor("mk-mic",P)`;

export class ShellAudio {
  spk = localStorage.getItem(LS) !== "0"; mic = false;
  ws = null; ctx = null; next = 0; fmt = { rate: 48000, ch: 2, in_rate: 48000, in_ch: 1 };
  micCtx = null; micStream = null; micNode = null; micBuf = []; micLen = 0; micPos = 0;
  listeners = new Set(); retry = 0; unavailable = false;

  constructor() {
    // Otomatik oynatma kuralı: ilk dokunuşta ses bağlamı başlatılır.
    const unlock = () => { if (this.spk) this.ensureCtx(); };
    addEventListener("pointerdown", unlock, true);
    addEventListener("keydown", unlock, true);
    this.sync();
  }

  on(fn) { this.listeners.add(fn); fn(this.state()); return () => this.listeners.delete(fn); }
  emit() { const s = this.state(); for (const f of this.listeners) f(s); }
  // waiting: hoparlör açık ama tarayıcı henüz ses çalmaya izin vermedi (sayfaya dokunulmadı).
  state() { return { spk: this.spk, mic: this.mic, waiting: this.spk && (!this.ctx || this.ctx.state !== "running"), unavailable: this.unavailable, secure: isSecureContext }; }

  setSpeaker(on) {
    this.spk = on; localStorage.setItem(LS, on ? "1" : "0");
    if (on) this.ensureCtx(); else this.next = 0;
    this.sync(); this.emit();
  }

  async setMic(on) {
    if (!on) { this.stopMic(); this.sync(); this.emit(); return; }
    if (!isSecureContext || !navigator.mediaDevices?.getUserMedia) throw Object.assign(new Error("insecure"), { code: "insecure" });
    // İzin penceresi burada açılır (kullanıcı tıklamasıyla).
    const stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
    try {
      const ctx = new AudioContext();
      await ctx.audioWorklet.addModule(URL.createObjectURL(new Blob([WORKLET], { type: "text/javascript" })));
      const src = ctx.createMediaStreamSource(stream), node = new AudioWorkletNode(ctx, "mk-mic");
      node.port.onmessage = e => this.micData(e.data, ctx.sampleRate);
      src.connect(node); // çıkışa bağlanmaz: kendi sesimizi duymayız
      this.micCtx = ctx; this.micStream = stream; this.micNode = node; this.micPos = 0;
      this.mic = true;
      stream.getAudioTracks()[0]?.addEventListener("ended", () => this.setMic(false));
    } catch (x) { stream.getTracks().forEach(t => t.stop()); throw x; }
    this.sync(); this.emit();
  }

  stopMic() {
    this.mic = false;
    this.micStream?.getTracks().forEach(t => t.stop());
    this.micNode?.disconnect(); this.micCtx?.close().catch(() => {});
    this.micCtx = this.micStream = this.micNode = null; this.micBuf = []; this.micLen = 0;
    this.send({ t: "mic", on: false });
  }

  // Mikrofon: cihaz hızından 48 kHz'e (doğrusal), 20 ms'lik parçalar hâlinde s16le.
  micData(f32, rate) {
    const out = this.fmt.in_rate;
    let res;
    if (rate === out) res = f32;
    else {
      // Önceki bloğun son örneği başa eklenir: bloklar arasında kesintisiz ara değerleme.
      const x = new Float32Array(f32.length + 1); x[0] = this.micPrev ?? f32[0]; x.set(f32, 1);
      const step = rate / out, r = [];
      let p = this.micPos;
      for (; p < x.length - 1; p += step) { const i = p | 0, k = p - i; r.push(x[i] * (1 - k) + x[i + 1] * k); }
      this.micPos = p - (x.length - 1); this.micPrev = x[x.length - 1];
      res = r;
    }
    this.micBuf.push(res); this.micLen += res.length;
    const N = out / 50;
    while (this.micLen >= N) {
      const pcm = new Int16Array(N); let o = 0;
      while (o < N) {
        const a = this.micBuf[0], take = Math.min(a.length, N - o);
        for (let j = 0; j < take; j++) { const v = Math.max(-1, Math.min(1, a[j])); pcm[o + j] = v < 0 ? v * 0x8000 : v * 0x7fff; }
        o += take;
        if (take === a.length) this.micBuf.shift(); else this.micBuf[0] = a.slice(take);
      }
      this.micLen -= N;
      if (this.ws?.readyState === 1 && this.ws.bufferedAmount < 64 << 10) this.ws.send(pcm.buffer);
    }
  }

  ensureCtx() {
    if (!this.ctx) {
      try { this.ctx = new AudioContext({ sampleRate: this.fmt.rate, latencyHint: "interactive" }); }
      catch { this.ctx = new AudioContext({ latencyHint: "interactive" }); }
      this.ctx.onstatechange = () => this.emit();
      this.gain = this.ctx.createGain(); this.gain.connect(this.ctx.destination);
    }
    if (this.ctx.state !== "running") this.ctx.resume().then(() => this.emit()).catch(() => {});
  }

  send(m) { if (this.ws?.readyState === 1) this.ws.send(JSON.stringify(m)); }

  // Hoparlör ya da mikrofon açıkken bağlı kal; ikisi de kapalıysa bağlantıyı kapat.
  sync() {
    const want = this.spk || this.mic;
    if (!want) { if (this.ws) { const w = this.ws; this.ws = null; w.onclose = null; w.close(); } return; }
    if (this.ws) { this.send({ t: "spk", on: this.spk }); if (this.mic) this.send({ t: "mic", on: true }); return; }
    const ws = this.ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/v1/audio`);
    ws.binaryType = "arraybuffer";
    ws.onopen = () => { this.retry = 0; this.send({ t: "spk", on: this.spk }); if (this.mic) this.send({ t: "mic", on: true }); };
    ws.onmessage = e => {
      if (typeof e.data === "string") {
        const m = JSON.parse(e.data);
        if (m.t === "fmt") { this.fmt = m; if (this.unavailable) { this.unavailable = false; this.emit(); } }
        else if (m.t === "error") { this.unavailable = true; this.emit(); }
      } else this.play(e.data);
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      // Ses desteği yoksa (eski sistem) seyrek dene.
      const wait = this.unavailable ? 60000 : Math.min(15000, 500 * 2 ** this.retry++);
      setTimeout(() => this.sync(), wait);
    };
  }

  play(buf) {
    if (!this.spk || !this.ctx || this.ctx.state !== "running") return;
    const { rate, ch } = this.fmt, s = new Int16Array(buf), n = (s.length / ch) | 0;
    if (!n) return;
    const now = this.ctx.currentTime;
    if (this.next < now) this.next = now + LEAD; // boşluk: yeniden tamponla
    else if (this.next - now > MAX_LAG) return; // gecikme birikti: parçayı atla
    const ab = this.ctx.createBuffer(ch, n, rate);
    for (let c = 0; c < ch; c++) {
      const d = ab.getChannelData(c);
      for (let i = 0; i < n; i++) d[i] = s[i * ch + c] / 32768;
    }
    const src = this.ctx.createBufferSource();
    src.buffer = ab; src.connect(this.gain); src.start(this.next);
    this.next += ab.duration;
  }
}
