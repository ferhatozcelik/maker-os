//! makeros-ai: Türkçe, internetsiz Linux asistanı (Qwen2.5-0.5B-Instruct Q4_K_M + llama.cpp).
//! Komutları yalnız önerir ve risk düzeyini gösterir; hiçbir komutu çalıştırmaz.

use makerd::ai::{cli, llama, server};
use makerd::{flags::Flags, logging};
use std::time::Duration;

const MODEL: &str = "/var/lib/makeros-ai/models/qwen2.5-0.5b-instruct-q4_k_m.gguf";
const MODEL_URL: &str = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";
const MODEL_SHA256: &str = "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db";

const USAGE: &str = "Kullanım: makeros-ai <komut> [seçenekler]

  sohbet [--mod M]         etkileşimli Türkçe sohbet (geçmiş tutulur)
  sor [--mod M] \"soru\"     tek soru; boru ile gelen metin eklenir:
                             journalctl -p err -b -n 20 | makeros-ai sor \"bu hataları açıkla\"
                           izin modu M: oner (yalnız öneri, varsayılan) · sor (her komut için sorar) ·
                           otomatik (SAFE komutlar sormadan; WARNING/DANGEROUS her zaman onayla).
                           Verilmezse Ayarlar'daki AI Asistan izin modu kullanılır.
  risk \"komut\"             komutun risk düzeyi (SAFE / WARNING / DANGEROUS); çıkış kodu 0/10/20
  durum                    hizmet ve model durumu (yüklü mü, RAM)
  olcum [--n 8] [--json] [--goster]
                           performans: model yükleme, ilk yanıt gecikmesi, token/sn, RAM, CPU
  model-kur [--dosya YOL]  modeli kur (internetten bir kez indirir ya da USB'deki dosyadan kopyalar;
                           SHA-256 doğrulanır)
  serve [--listen 127.0.0.1:8091] [--model YOL] [--llama llama-server] [--ctx 2048]
        [--threads 4] [--port 8092] [--idle 5m] [--cache-dir /var/cache/makeros-ai] [--preload]
                           OpenAI uyumlu API hizmeti (systemd: makeros-ai.service)

Ortam: MAKEROS_AI_URL (istemciler için, varsayılan http://127.0.0.1:8091)";

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    let code = match cmd.as_str() {
        "serve" => serve(rest),
        "sohbet" | "chat" | "sor" | "ask" => {
            // --mod oner|sor|otomatik (yoksa kullanıcı tercihi aiMode, o da yoksa yalnız öneri)
            let (mut m, mut words) = (String::new(), Vec::new());
            let mut it = rest.iter();
            while let Some(w) = it.next() {
                if w == "--mod" { m = it.next().cloned().unwrap_or_default() } else if let Some(v) = w.strip_prefix("--mod=") { m = v.into() } else { words.push(w.clone()) }
            }
            let mode = cli::mode_from(&m);
            if cmd == "sohbet" || cmd == "chat" { cli::chat(mode) } else { cli::one(&words.join(" "), mode) }
        }
        "risk" => {
            if rest.is_empty() { eprintln!("kullanım: makeros-ai risk \"komut\""); 2 } else { cli::risk(&rest.join(" ")) }
        }
        "durum" | "status" => cli::status(),
        "olcum" | "ölçüm" | "bench" => match Flags::parse(rest.into_iter(), &["n"], &["json", "goster"]) {
            Ok(f) => cli::bench(f.int("n", 8).unwrap_or(8).clamp(1, 20) as usize, f.bool("json"), f.bool("goster")),
            Err(e) => { eprintln!("{e}\n\n{USAGE}"); 2 }
        },
        "model-kur" => match Flags::parse(rest.into_iter(), &["dosya", "model"], &[]) {
            Ok(f) => install_model(&f.str("dosya", ""), &f.str("model", MODEL)),
            Err(e) => { eprintln!("{e}\n\n{USAGE}"); 2 }
        },
        "--version" | "-V" => { println!("makeros-ai {}", env!("CARGO_PKG_VERSION")); 0 }
        _ => { eprintln!("{USAGE}"); if cmd.is_empty() || cmd == "-h" || cmd == "--help" || cmd == "yardim" { 0 } else { 2 } }
    };
    std::process::exit(code);
}

fn serve(rest: Vec<String>) -> i32 {
    let f = match Flags::parse(rest.into_iter(), &["listen", "model", "llama", "ctx", "threads", "port", "idle", "log-level", "cache-dir"], &["preload"]) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}\n\n{USAGE}"); return 2 }
    };
    logging::init(&f.str("log-level", "info"), "text");
    let threads = std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(4);
    let cfg = llama::Config {
        bin: f.str("llama", "llama-server"),
        model: f.str("model", MODEL),
        ctx: f.int("ctx", 2048).unwrap_or(2048).clamp(512, 32768) as u32,
        threads: f.int("threads", threads).unwrap_or(threads).clamp(1, 16) as u32,
        port: f.int("port", 8092).unwrap_or(8092) as u16,
        idle: f.duration("idle", Duration::from_secs(300)).unwrap_or(Duration::from_secs(300)),
        cache_dir: {
            let d = f.str("cache-dir", "/var/cache/makeros-ai");
            // Yazılabilir değilse önbellek kapalı (her soğuk başlangıçta önek yeniden hesaplanır).
            let probe = std::path::Path::new(&d).join(".yazma-denemesi");
            if std::fs::write(&probe, b"").is_ok() { let _ = std::fs::remove_file(&probe); d } else { String::new() }
        },
    };
    let listen = f.str("listen", "127.0.0.1:8091");
    let preload = f.bool("preload");
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio");
    rt.block_on(async move {
        let l = llama::Llama::new(cfg.clone());
        if preload {
            match l.ensure().await {
                Ok(_) => server::prime(&l).await,
                Err(e) => tracing::warn!("ön yükleme: {e}"),
            }
        } else if !cfg.cache_dir.is_empty() && !std::path::Path::new(&cfg.cache_dir).join(server::prefix_file(&l)).is_file() {
            // Önek önbelleği yoksa (ilk kurulum, istem değişti) arka planda bir kez hazırla; model sonra boşta atılır.
            let l2 = l.clone();
            tokio::spawn(async move {
                if l2.ensure().await.is_ok() {
                    server::prime(&l2).await;
                }
            });
        }
        let app = server::router(l.clone());
        let ln = match tokio::net::TcpListener::bind(&listen).await {
            Ok(x) => x,
            Err(e) => { eprintln!("{listen} dinlenemiyor: {e}"); return 1 }
        };
        tracing::info!(%listen, model = %cfg.model, ctx = cfg.ctx, threads = cfg.threads, idle_s = cfg.idle.as_secs(), "makeros-ai hazır");
        let stop = async {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("sinyal");
            tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
        };
        let r = axum::serve(ln, app).with_graceful_shutdown(stop).await;
        l.stop().await;
        if let Err(e) = r { eprintln!("{e}"); 1 } else { 0 }
    })
}

/// Modeli kurar: `--dosya` verilirse oradan kopyalar (internetsiz kurulum), yoksa bir kez indirir. SHA-256 doğrulanır.
fn install_model(src: &str, dst: &str) -> i32 {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    if std::path::Path::new(dst).is_file() && sha_file(dst).as_deref() == Some(MODEL_SHA256) {
        println!("model zaten kurulu ve doğrulandı: {dst}");
        return 0;
    }
    if let Some(d) = std::path::Path::new(dst).parent() {
        if let Err(e) = std::fs::create_dir_all(d) { eprintln!("{}: {e} (sudo ile çalıştırın)", d.display()); return 1 }
    }
    let part = format!("{dst}.part");
    let mut out = match std::fs::File::create(&part) { Ok(f) => f, Err(e) => { eprintln!("{part}: {e} (sudo ile çalıştırın)"); return 1 } };
    let mut reader: Box<dyn Read> = if src.is_empty() {
        println!("indiriliyor: {MODEL_URL}");
        let ag: ureq::Agent = ureq::Agent::config_builder().timeout_connect(Some(Duration::from_secs(20))).build().into();
        match ag.get(MODEL_URL).call() { Ok(r) => Box::new(r.into_body().into_reader()), Err(e) => { eprintln!("indirilemedi: {e}"); return 1 } }
    } else {
        match std::fs::File::open(src) { Ok(f) => Box::new(f), Err(e) => { eprintln!("{src}: {e}"); return 1 } }
    };
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let k = match reader.read(&mut buf) { Ok(0) => break, Ok(k) => k, Err(e) => { eprintln!("okuma hatası: {e}"); return 1 } };
        h.update(&buf[..k]);
        if let Err(e) = out.write_all(&buf[..k]) { eprintln!("yazma hatası: {e}"); return 1 }
        n += k as u64;
        if n % (32 << 20) < k as u64 { eprint!("\r{} MB", n >> 20); }
    }
    eprintln!("\r{} MB", n >> 20);
    let got = hex::encode(h.finalize());
    if got != MODEL_SHA256 {
        let _ = std::fs::remove_file(&part);
        eprintln!("SHA-256 uyuşmuyor: {got} (beklenen {MODEL_SHA256})");
        return 1;
    }
    if let Err(e) = std::fs::rename(&part, dst) { eprintln!("{e}"); return 1 }
    let _ = std::fs::set_permissions(dst, std::os::unix::fs::PermissionsExt::from_mode(0o644));
    // Model lisansı (Apache-2.0) modelin yanına: USB'den kurulumda kaynağın yanındaki LICENSE, yoksa indirilir.
    let lic = std::path::Path::new(dst).with_file_name("LICENSE.qwen");
    let local = std::path::Path::new(src).with_file_name("LICENSE");
    if !src.is_empty() && local.is_file() {
        let _ = std::fs::copy(&local, &lic);
    } else if src.is_empty() {
        let url = MODEL_URL.rsplit_once('/').map(|(b, _)| format!("{b}/LICENSE")).unwrap_or_default();
        if let Ok(mut r) = ureq::get(&url).call() {
            if let Ok(t) = r.body_mut().read_to_string() { let _ = std::fs::write(&lic, t); }
        }
    }
    println!("model kuruldu ve doğrulandı: {dst}");
    0
}

fn sha_file(p: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let k = f.read(&mut buf).ok()?;
        if k == 0 { break }
        h.update(&buf[..k]);
    }
    Some(hex::encode(h.finalize()))
}
