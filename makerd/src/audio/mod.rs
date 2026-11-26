//! Kullanıcı programlarının sesi ↔ tarayıcı (hoparlör ve mikrofon).
//!
//! makerd-user kullanıcıya ait bir PulseAudio sunucusu çalıştırır (`$XDG_RUNTIME_DIR/pulse/native`).
//! Sunucunun tek çıkışı "MakerOS" adlı bir boru (FIFO) çıkışıdır, tek girişi "Mikrofon" adlı bir boru
//! girişidir: programların (pygame/SDL, PySide6/Qt, sounddevice/PortAudio, aplay, espeak, Chromium)
//! çaldığı ses borudan okunur ve hoparlörü açık her tarayıcıya ham PCM olarak gönderilir; mikrofonu
//! açık tarayıcının sesi giriş borusuna yazılır. ALSA kullanan programlar `$XDG_RUNTIME_DIR/makeros-asound.conf`
//! ile (bkz. /etc/asound.conf) PulseAudio'ya yönlenir. Pi'nin kendi ekranındaki kabuk da bir tarayıcıdır:
//! hoparlörü açıkken ses HDMI/3,5 mm'den (ya da Bluetooth kulaklıktan) çıkar.
//!
//! Biçim: çıkış 48 kHz 16 bit stereo, giriş 48 kHz 16 bit mono (küçük sonlu, araya geçmeli).

pub mod stream;

use bytes::Bytes;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// PulseAudio sunucusu.
pub const PULSEAUDIO: &str = "/usr/bin/pulseaudio";
pub const RATE: u32 = 48_000;
pub const OUT_CH: u32 = 2;
pub const IN_CH: u32 = 1;
/// 20 ms'lik parça (bayt): çıkış 3840, giriş 1920.
const OUT_CHUNK: usize = (RATE as usize / 50) * OUT_CH as usize * 2;
const IN_CHUNK: usize = (RATE as usize / 50) * IN_CH as usize * 2;

/// Kullanıcının ses ortamı (henüz başlatılmamış).
#[derive(Clone, Debug)]
pub struct Setup {
    /// $XDG_RUNTIME_DIR
    pub runtime: PathBuf,
    pub pulseaudio: PathBuf,
}

impl Setup {
    /// PulseAudio kuruluysa kullanıcının ses ortamını tanımlar.
    pub fn for_runtime(runtime: &Path) -> Option<Self> {
        let pa = std::env::var_os("MAKEROS_PULSEAUDIO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(PULSEAUDIO));
        pa.is_file().then(|| Self { runtime: runtime.to_path_buf(), pulseaudio: pa })
    }

    fn dir(&self) -> PathBuf {
        self.runtime.join("makeros-audio")
    }

    /// Kullanıcı programlarına verilecek ortam.
    pub fn env(&self) -> Vec<(&'static str, String)> {
        vec![("PULSE_SERVER", format!("unix:{}", self.runtime.join("pulse/native").display()))]
    }
}

/// Çalışan ses köprüsü: çıkış yayını ve mikrofon girişi.
pub struct Sound {
    out: broadcast::Sender<Bytes>,
    mic: std::sync::mpsc::SyncSender<(u64, Bytes)>,
    /// Mikrofonu en son açan istemci (aynı anda tek mikrofon; sonuncusu geçerli).
    mic_owner: AtomicU64,
    next_id: AtomicU64,
}

impl Sound {
    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.out.subscribe()
    }

    pub fn new_client(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn mic_on(&self, id: u64) {
        self.mic_owner.store(id, Ordering::Relaxed);
    }

    pub fn mic_off(&self, id: u64) {
        let _ = self.mic_owner.compare_exchange(id, 0, Ordering::Relaxed, Ordering::Relaxed);
    }

    /// Mikrofon PCM'i (yalnız mikrofonu açık istemciden kabul edilir; dolu kuyrukta atılır).
    pub fn mic_data(&self, id: u64, pcm: Bytes) {
        if id != 0 && self.mic_owner.load(Ordering::Relaxed) == id {
            let _ = self.mic.try_send((id, pcm));
        }
    }
}

static LOCAL: std::sync::OnceLock<Arc<Sound>> = std::sync::OnceLock::new();

/// Süreç içi köprü (yalnız mock önizlemede makerd kendisi başlatır).
pub fn local() -> Option<Arc<Sound>> {
    LOCAL.get().cloned()
}

pub fn set_local(s: Arc<Sound>) {
    let _ = LOCAL.set(s);
}

/// PulseAudio'yu çalıştırır, çıkış borusunu okur ve mikrofon borusuna yazar (ayrı iş parçacıkları).
pub fn start(s: Setup) -> Arc<Sound> {
    let (out, _) = broadcast::channel(64);
    let (mtx, mrx) = std::sync::mpsc::sync_channel::<(u64, Bytes)>(32);
    let sound = Arc::new(Sound { out: out.clone(), mic: mtx, mic_owner: AtomicU64::new(0), next_id: AtomicU64::new(0) });
    let dir = s.dir();
    if let Err(e) = prepare(&s) {
        tracing::warn!(err = %e, "ses ortamı hazırlanamadı");
    }
    let s2 = s.clone();
    std::thread::Builder::new()
        .name("ses".into())
        .spawn(move || {
            let mut wait = Duration::from_millis(500);
            loop {
                let started = Instant::now();
                match run_daemon(&s2, &out) {
                    Ok(()) => {}
                    Err(e) => tracing::warn!(err = %e, "PulseAudio durdu"),
                }
                if started.elapsed() > Duration::from_secs(30) {
                    wait = Duration::from_millis(500);
                }
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_secs(15));
            }
        })
        .expect("ses iş parçacığı");
    let snd = sound.clone();
    std::thread::Builder::new().name("mikrofon".into()).spawn(move || mic_pump(&dir.join("in"), mrx, &snd)).expect("mikrofon iş parçacığı");
    sound
}

/// Sunucu betiği ve ALSA yönlendirmesi.
fn prepare(s: &Setup) -> std::io::Result<()> {
    let dir = s.dir();
    std::fs::create_dir_all(&dir)?;
    let (out, inp) = (dir.join("out"), dir.join("in"));
    let pa = format!(
        "# makerd-user tarafından yazıldı (bkz. makerd/src/audio).\n\
         load-module module-native-protocol-unix\n\
         load-module module-pipe-sink file={out} sink_name=makeros format=s16le rate={RATE} channels={OUT_CH} use_system_clock_for_timing=yes sink_properties=device.description=MakerOS\n\
         load-module module-pipe-source file={inp} source_name=makeros_mic format=s16le rate={RATE} channels={IN_CH} source_properties=device.description=Mikrofon\n\
         set-default-sink makeros\n\
         set-default-source makeros_mic\n",
        out = out.display(),
        inp = inp.display()
    );
    write_atomic(&dir.join("default.pa"), pa.as_bytes())?;
    // /etc/asound.conf bu dosyayı (varsa) yükler: ALSA programları PulseAudio'ya gider.
    write_atomic(&s.runtime.join("makeros-asound.conf"), b"# makerd-user: ALSA -> PulseAudio (tarayici hoparlor/mikrofon)\npcm.!default { type pulse }\nctl.!default { type pulse }\n")
}

fn write_atomic(p: &Path, b: &[u8]) -> std::io::Result<()> {
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, b)?;
    std::fs::rename(&tmp, p)
}

/// Sunucuyu başlatır ve çıkış borusunu süreç yaşadıkça okur.
fn run_daemon(s: &Setup, out: &broadcast::Sender<Bytes>) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    prepare(s)?;
    let dir = s.dir();
    let mut cmd = std::process::Command::new(&s.pulseaudio);
    cmd.args(["-n", "-F"])
        .arg(dir.join("default.pa"))
        .args(["--daemonize=no", "--exit-idle-time=-1", "--use-pid-file=no", "--system=no", "--realtime=no", "--high-priority=no", "--log-target=stderr", "--log-level=warning"])
        .env("XDG_RUNTIME_DIR", &s.runtime)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    // SAFETY: fork sonrası yalnız async-signal-safe prctl çağrılır.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        })
    };
    let mut c = cmd.spawn()?;
    if let Some(err) = c.stderr.take() {
        std::thread::spawn(move || {
            use std::io::BufRead;
            for l in std::io::BufReader::new(err).lines().map_while(Result::ok).take(60) {
                if !l.trim().is_empty() {
                    tracing::info!(line = %l, "PulseAudio");
                }
            }
        });
    }
    // Boruyu sunucu oluşturur (mkfifo); hazır olmasını bekle.
    let fifo = dir.join("out");
    let mut f = None;
    for _ in 0..100 {
        if let Ok(Some(st)) = c.try_wait() {
            return Err(std::io::Error::other(format!("pulseaudio çıktı: {st}")));
        }
        if is_fifo(&fifo) {
            // O_RDWR: sunucu yeniden başlarken EOF gelmesin.
            if let Ok(x) = std::fs::OpenOptions::new().read(true).write(true).open(&fifo) {
                f = Some(x);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let Some(mut f) = f else {
        let _ = c.kill();
        let _ = c.wait();
        return Err(std::io::Error::other("PulseAudio çıkış borusu hazır olmadı"));
    };
    tracing::info!(pid = c.id(), "PulseAudio hazır (tarayıcı sesi)");
    // Okuyucu ayrı iş parçacığında; süreç ölünce boruyu kapatıp döngüden çıkarız.
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (out2, done2) = (out.clone(), done.clone());
    let reader = std::thread::spawn(move || {
        let mut buf = vec![0u8; OUT_CHUNK * 4];
        let mut rest: Vec<u8> = Vec::new();
        while !done2.load(Ordering::Relaxed) {
            match f.read(&mut buf) {
                Ok(0) => std::thread::sleep(Duration::from_millis(10)),
                Ok(n) => {
                    rest.extend_from_slice(&buf[..n]);
                    // Tam kareler (4 bayt) gönderilir.
                    let whole = rest.len() / 4 * 4;
                    if whole >= OUT_CHUNK || (whole > 0 && n < buf.len()) {
                        let chunk: Vec<u8> = rest.drain(..whole).collect();
                        // Sessizlik gönderilmez (boşta 192 KB/sn yerine 0); tarayıcı boşluğu kendisi doldurur.
                        if out2.receiver_count() > 0 && chunk.iter().any(|&b| b != 0) {
                            let _ = out2.send(Bytes::from(chunk));
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    });
    let st = c.wait();
    done.store(true, Ordering::Relaxed);
    // Okuyucu read()'de bekliyorsa uyandır: boruya (O_RDWR açıldığı için) bir kare yaz.
    if let Ok(mut w) = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&fifo) {
        let _ = w.write_all(&[0u8; 4]);
    }
    let _ = reader.join();
    Err(std::io::Error::other(format!("pulseaudio çıktı: {}", st.map(|s| s.to_string()).unwrap_or_default())))
}

fn is_fifo(p: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::symlink_metadata(p).map(|m| m.file_type().is_fifo()).unwrap_or(false)
}

/// Mikrofon borusunu gerçek zamanlı besler: mikrofon verisi yoksa sessizlik yazılır (kayıt yapan
/// programlar beklemede kalmasın). Boru doluysa (kimse okumuyor) veri atılır.
fn mic_pump(fifo: &Path, rx: std::sync::mpsc::Receiver<(u64, Bytes)>, snd: &Sound) {
    let silence = vec![0u8; IN_CHUNK];
    let mut f: Option<std::fs::File> = None;
    let mut next = Instant::now();
    let mut last_mic = Instant::now() - Duration::from_secs(10);
    loop {
        let timeout = next.saturating_duration_since(Instant::now());
        let data = match rx.recv_timeout(timeout) {
            Ok((id, b)) if snd.mic_owner.load(Ordering::Relaxed) == id => {
                last_mic = Instant::now();
                Some(b)
            }
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if f.is_none() && is_fifo(fifo) {
            f = std::fs::OpenOptions::new().read(true).write(true).custom_flags(libc::O_NONBLOCK).open(fifo).ok();
        }
        let now = Instant::now();
        match data {
            Some(b) => {
                if let Some(w) = f.as_mut() {
                    write_some(w, &b);
                }
            }
            None => {
                // Mikrofon sustuysa (100 ms) 20 ms'lik sessizlik; zamanlama kaymasın diye hedef saate göre.
                if now.duration_since(last_mic) > Duration::from_millis(100) {
                    if let Some(w) = f.as_mut() {
                        write_some(w, &silence);
                    }
                }
                next += Duration::from_millis(20);
                if next + Duration::from_millis(200) < now {
                    next = now + Duration::from_millis(20);
                }
            }
        }
        if !is_fifo(fifo) {
            f = None; // sunucu yeniden başladı: boru yeniden oluşturulacak
        }
    }
}

/// Engellemeden yazar; boru doluysa (kimse kaydetmiyor) veri atılır.
fn write_some(w: &mut std::fs::File, b: &[u8]) {
    let _ = w.write(b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_writes_config() {
        let rd = std::env::temp_dir().join(format!("mk-audio-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&rd).unwrap();
        let s = Setup { runtime: rd.clone(), pulseaudio: PathBuf::from("/bin/true") };
        prepare(&s).unwrap();
        let pa = std::fs::read_to_string(rd.join("makeros-audio/default.pa")).unwrap();
        assert!(pa.contains("module-pipe-sink") && pa.contains("rate=48000 channels=2") && pa.contains("set-default-source makeros_mic"));
        assert!(std::fs::read_to_string(rd.join("makeros-asound.conf")).unwrap().contains("type pulse"));
        assert_eq!(s.env()[0].1, format!("unix:{}/pulse/native", rd.display()));
        let _ = std::fs::remove_dir_all(&rd);
    }

    #[test]
    fn mic_only_from_owner() {
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        let (out, _) = broadcast::channel(4);
        let s = Sound { out, mic: tx, mic_owner: AtomicU64::new(0), next_id: AtomicU64::new(0) };
        let (a, b) = (s.new_client(), s.new_client());
        s.mic_data(a, Bytes::from_static(b"x"));
        assert!(rx.try_recv().is_err(), "mikrofon kapalıyken atılır");
        s.mic_on(a);
        s.mic_on(b); // sonuncusu geçerli
        s.mic_data(a, Bytes::from_static(b"a"));
        s.mic_data(b, Bytes::from_static(b"b"));
        assert_eq!(rx.try_recv().unwrap().1, Bytes::from_static(b"b"));
        assert!(rx.try_recv().is_err());
        s.mic_off(a); // başkasının mikrofonunu kapatamaz
        s.mic_data(b, Bytes::from_static(b"c"));
        assert!(rx.try_recv().is_ok());
    }
}
