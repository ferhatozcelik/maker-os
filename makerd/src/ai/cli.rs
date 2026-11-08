//! Terminal istemcisi: sohbet, tek soru, risk, durum ve performans ölçümü. Komut ÇALIŞTIRMAZ.

use super::risk::{Assessment, Risk};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::time::{Duration, Instant};

pub fn base_url() -> String {
    std::env::var("MAKEROS_AI_URL").unwrap_or_else(|_| "http://127.0.0.1:8091".into())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).http_status_as_error(false).build().into()
}

struct Color(bool);
impl Color {
    fn c(&self, code: &str, s: &str) -> String {
        if self.0 { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
    }
}

fn color() -> Color {
    Color(std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

/// Bir yanıtın sonuç bilgileri.
#[derive(Default)]
pub struct Turn {
    pub text: String,
    pub commands: Vec<Assessment>,
    pub ttft_ms: f64,
    pub total_ms: f64,
    pub timings: Value,
    pub context: Value,
}

/// Akışlı istek; `on_text` her parça için çağrılır.
pub fn ask(messages: &[Value], max_tokens: u32, mut on_text: impl FnMut(&str)) -> Result<Turn, String> {
    let url = format!("{}/v1/chat/completions", base_url());
    let t0 = Instant::now();
    let mut res = agent()
        .post(&url)
        .header("Content-Type", "application/json")
        .send(json!({"model": super::server::MODEL_ID, "messages": messages, "stream": true, "max_tokens": max_tokens}).to_string())
        .map_err(|e| format!("asistan hizmetine ulaşılamadı ({e}). Başlatmak için: sudo systemctl start makeros-ai"))?;
    if !res.status().is_success() {
        let s = res.status();
        let b = res.body_mut().read_to_string().unwrap_or_default();
        let m = serde_json::from_str::<Value>(&b).ok().and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(String::from)).unwrap_or(b);
        return Err(format!("hata {s}: {m}"));
    }
    let mut turn = Turn::default();
    let r = BufReader::new(res.into_body().into_reader());
    for line in r.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(d) = line.strip_prefix("data: ") else { continue };
        if d == "[DONE]" { break }
        let Ok(v) = serde_json::from_str::<Value>(d) else { continue };
        if let Some(x) = v.get("x_makeros") {
            turn.commands = serde_json::from_value::<Vec<Value>>(x["commands"].clone()).unwrap_or_default().into_iter().filter_map(to_assessment).collect();
            turn.context = x["context"].clone();
        }
        if let Some(t) = v.get("timings") { turn.timings = t.clone(); }
        if let Some(c) = v.pointer("/choices/0/delta/content").and_then(Value::as_str) {
            if turn.text.is_empty() && !c.is_empty() { turn.ttft_ms = t0.elapsed().as_secs_f64() * 1000.0; }
            turn.text.push_str(c);
            on_text(c);
        }
    }
    turn.total_ms = t0.elapsed().as_secs_f64() * 1000.0;
    Ok(turn)
}

fn to_assessment(v: Value) -> Option<Assessment> {
    let risk = match v.get("risk")?.as_str()? { "SAFE" => Risk::Safe, "WARNING" => Risk::Warning, _ => Risk::Dangerous };
    Some(Assessment { command: v.get("command")?.as_str()?.to_string(), risk, reasons: v.get("reasons").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() })
}

pub fn print_commands(cmds: &[Assessment]) {
    print_commands_mode(cmds, super::exec::Mode::Suggest)
}

fn print_commands_mode(cmds: &[Assessment], mode: super::exec::Mode) {
    if cmds.is_empty() { return }
    let c = color();
    println!("\n{}", c.c("1", "── Önerilen komutlar ──"));
    for a in cmds { print_assessment(&c, a); }
    if mode == super::exec::Mode::Suggest {
        println!("{}", c.c("2", "Komutlar çalıştırılmadı (izin modu: yalnız öneri). İnceleyip kendiniz çalıştırın."));
    }
}

fn print_assessment(c: &Color, a: &Assessment) {
    let tag = match a.risk { Risk::Safe => c.c("1;32", " SAFE "), Risk::Warning => c.c("1;33", " WARNING "), Risk::Dangerous => c.c("1;31", " DANGEROUS ") };
    println!("{tag} {}  {}", c.c("1", &a.command), c.c("2", &format!("— {}", a.risk.tr())));
    for r in &a.reasons { println!("        · {r}"); }
}

/// İzin modu: `--mod` verilmezse kullanıcı tercihi (~/.config/makeros/prefs.json → aiMode), yoksa yalnız öneri.
pub fn mode_from(flag: &str) -> super::exec::Mode {
    if !flag.is_empty() {
        return super::exec::Mode::parse(flag);
    }
    let p = std::env::var("HOME").map(|h| format!("{h}/{}", crate::prefs::HOME_PATH)).unwrap_or_default();
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.get("aiMode").and_then(Value::as_str).map(super::exec::Mode::parse)).unwrap_or(super::exec::Mode::Suggest)
}

fn ask_line(prompt: &str) -> String {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim().to_string()
}

/// Önerilen komutları moda göre çalıştırır (onay sorar). Dönüş: (komut, çıktı) — modele açıklatmak için.
pub fn run_commands(mode: super::exec::Mode, cmds: &[Assessment]) -> Vec<(String, String)> {
    use super::exec::{self, Decision};
    let c = color();
    let mut done = Vec::new();
    if mode == exec::Mode::Suggest || cmds.is_empty() {
        return done;
    }
    for a in cmds {
        let mut d = exec::decide(mode, a, false, None);
        if let Decision::Confirm(r) = d {
            let q = match r {
                Risk::Safe => format!("{} çalıştırılsın mı? [e/H] ", c.c("1", &a.command)),
                Risk::Warning => format!("{} {} sistemi değiştirebilir. Çalıştırılsın mı? [e/H] ", c.c("1;33", "DİKKAT"), c.c("1", &a.command)),
                Risk::Dangerous => format!("{} {} geri alınamayabilir. Çalıştırmak için komutu aynen yazın (boş: vazgeç):\n› ", c.c("1;31", "TEHLİKELİ"), c.c("1", &a.command)),
            };
            let ans = ask_line(&q);
            d = if r == Risk::Dangerous { exec::decide(mode, a, true, Some(&ans)) } else if matches!(ans.to_lowercase().as_str(), "e" | "evet" | "y" | "yes") { Decision::Run } else { Decision::Deny("vazgeçildi".into()) };
        }
        match d {
            Decision::Run => {
                println!("{} {}", c.c("1;36", "$"), a.command);
                let out = std::process::Command::new("sh").arg("-c").arg(exec::wrap(&a.command)).output();
                match out {
                    Ok(o) => {
                        let (text, code, _) = exec::parse_output(&o.stdout);
                        if !text.is_empty() { println!("{text}"); }
                        let tag = match code { Some(0) => c.c("32", "çıkış 0"), Some(n) => c.c("31", &format!("çıkış {n}")), None => c.c("33", "çıkış ?") };
                        println!("{}{}", tag, exec::note(code, &text).map(|n| format!(" · {n}")).unwrap_or_default());
                        tracing::info!(risk = a.risk.label(), code = ?code, cmd = %a.command, "AI asistan komutu çalıştırıldı (terminal)");
                        done.push((a.command.clone(), text));
                    }
                    Err(e) => eprintln!("{}", c.c("31", &format!("çalıştırılamadı: {e}"))),
                }
            }
            Decision::Deny(why) => println!("{}", c.c("2", &format!("çalıştırılmadı: {why}"))),
            Decision::Confirm(_) => println!("{}", c.c("2", "çalıştırılmadı: komut aynen yazılmadı")),
        }
    }
    done
}

/// Komut çıktısını modele açıklatma iletisi (kısa tutulur: Pi'de istem işleme yavaştır).
pub fn explain_prompt(cmd: &str, out: &str) -> String {
    let mut o: String = out.chars().take(1200).collect();
    if out.chars().count() > 1200 { o.push_str("\n…"); }
    format!("Şu komutu çalıştırdım: `{cmd}`\nÇıktı:\n```\n{o}\n```\nÇıktıyı kısaca Türkçe açıkla; sorun varsa belirt.")
}

/// Etkileşimli sohbet (geçmiş oturum boyunca tutulur).
pub fn chat(mode: super::exec::Mode) -> i32 {
    let c = color();
    println!("{}", c.c("1", "MakerOS Türkçe Linux asistanı (yerel, internetsiz · Qwen2.5-0.5B)"));
    let mtxt = match mode {
        super::exec::Mode::Suggest => "izin modu: yalnız öneri (komut çalıştırılmaz; --mod sor|otomatik)",
        super::exec::Mode::Ask => "izin modu: izin iste (her komut için sorulur)",
        super::exec::Mode::Auto => "izin modu: otomatik (güvenli komutlar sormadan; diğerleri onayla)",
    };
    println!("{}", c.c("2", &format!("{mtxt}. /yeni: geçmişi sil · /risk <komut> · /cik: çıkış")));
    let mut history: Vec<Value> = Vec::new();
    let stdin = std::io::stdin();
    loop {
        print!("\n{} ", c.c("1;36", "siz ›"));
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 { println!(); return 0 }
        let q = line.trim();
        if q.is_empty() { continue }
        match q {
            "/cik" | "/çık" | "/q" | "exit" | "quit" => return 0,
            "/yeni" => { history.clear(); println!("{}", c.c("2", "geçmiş silindi")); continue }
            _ => {}
        }
        if let Some(cmd) = q.strip_prefix("/risk ") { print_assessment(&c, &super::risk::classify(cmd)); continue }
        history.push(json!({"role": "user", "content": q}));
        print!("{} ", c.c("1;35", "asistan ›"));
        let _ = std::io::stdout().flush();
        match ask(&history, 512, |t| { print!("{t}"); let _ = std::io::stdout().flush(); }) {
            Ok(t) => {
                println!();
                print_commands_mode(&t.commands, mode);
                let tps = t.timings.get("predicted_per_second").and_then(Value::as_f64).unwrap_or(0.0);
                println!("{}", c.c("2", &format!("ilk yanıt {:.1} sn · {:.1} token/sn", t.ttft_ms / 1000.0, tps)));
                history.push(json!({"role": "assistant", "content": t.text}));
                let ran = run_commands(mode, &t.commands);
                if let Some((cmd, out)) = ran.last() {
                    if matches!(ask_line(&c.c("2", "Çıktıyı asistana açıklatayım mı? [e/H] ")).to_lowercase().as_str(), "e" | "evet") {
                        history.push(json!({"role": "user", "content": explain_prompt(cmd, out)}));
                        print!("{} ", c.c("1;35", "asistan ›"));
                        match ask(&history, 400, |t| { print!("{t}"); let _ = std::io::stdout().flush(); }) {
                            Ok(t) => { println!(); history.push(json!({"role": "assistant", "content": t.text})); }
                            Err(e) => { history.pop(); eprintln!("\n{}", c.c("31", &e)); }
                        }
                    }
                }
            }
            Err(e) => { history.pop(); eprintln!("\n{}", c.c("31", &e)); }
        }
    }
}

/// Tek soru. stdin bir boru ise (ör. `journalctl -p err | makeros-ai sor "açıkla"`) içeriği soruya eklenir.
pub fn one(question: &str, mode: super::exec::Mode) -> i32 {
    let mut q = question.to_string();
    if !std::io::stdin().is_terminal() {
        let mut s = String::new();
        let _ = std::io::stdin().take(64 * 1024).read_to_string(&mut s);
        if !s.trim().is_empty() { q = format!("{q}\n```\n{}\n```", s.trim_end()); }
    }
    if q.trim().is_empty() { eprintln!("kullanım: makeros-ai sor \"soru\""); return 2 }
    match ask(&[json!({"role": "user", "content": q})], 512, |t| { print!("{t}"); let _ = std::io::stdout().flush(); }) {
        Ok(t) => {
            println!();
            print_commands_mode(&t.commands, mode);
            // Boru ile gelen girdide onay sorulamaz: yalnız sormadan çalışabilenler (otomatik + SAFE).
            if std::io::stdin().is_terminal() || mode == super::exec::Mode::Auto {
                let cmds: Vec<Assessment> = if std::io::stdin().is_terminal() { t.commands.clone() } else { t.commands.iter().filter(|a| a.risk == Risk::Safe).cloned().collect() };
                run_commands(mode, &cmds);
            }
            0
        }
        Err(e) => { eprintln!("{e}"); 1 }
    }
}

pub fn risk(cmd: &str) -> i32 {
    let a = super::risk::classify(cmd);
    print_assessment(&color(), &a);
    match a.risk { Risk::Safe => 0, Risk::Warning => 10, Risk::Dangerous => 20 }
}

pub fn status() -> i32 {
    match agent().get(format!("{}/v1/makeros/durum", base_url())).call() {
        Ok(mut r) => { println!("{}", r.body_mut().read_to_string().unwrap_or_default()); 0 }
        Err(e) => { eprintln!("asistan hizmetine ulaşılamadı: {e}"); 1 }
    }
}

// --- Performans ölçümü ----------------------------------------------------------------------------

fn cpu_jiffies() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let v: Vec<u64> = s.lines().next().unwrap_or("").split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
    (v.iter().sum(), idle)
}

fn temp_c() -> f64 {
    std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok().and_then(|s| s.trim().parse::<f64>().ok()).map(|m| m / 1000.0).unwrap_or(f64::NAN)
}

fn mem_available_mb() -> f64 {
    let s = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    s.lines().find(|l| l.starts_with("MemAvailable:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<f64>().ok()).map(|k| k / 1024.0).unwrap_or(f64::NAN)
}

fn get_json(path: &str) -> Value {
    agent().get(format!("{}{path}", base_url())).call().ok().and_then(|mut r| r.body_mut().read_to_string().ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

const BENCH: &[&str] = &[
    "CPU sıcaklığını göster.",
    "RAM kullanımını kontrol et.",
    "Disk alanını kontrol et.",
    "Çalışan servisleri listele.",
    "IP adresimi öğren.",
    "Son sistem hatalarını göster.",
    "Şu hatayı açıkla: E: Could not get lock /var/lib/dpkg/lock-frontend. It is held by process 1234 (apt)",
    "/home/maker/projeler klasörünün boyutunu nasıl öğrenirim?",
];

/// Ölçüm: soğuk başlatma (model yükleme), her soru için ilk yanıt gecikmesi, token/sn, RAM ve CPU.
pub fn bench(n: usize, json_out: bool, show: bool) -> i32 {
    let c = color();
    // Soğuk başlangıç için modeli boşalt.
    let _ = agent().post(format!("{}/v1/makeros/bosalt", base_url())).send_empty();
    std::thread::sleep(Duration::from_millis(500));
    let mem_before = mem_available_mb();
    let qs: Vec<&str> = BENCH.iter().copied().take(n.max(1)).collect();
    let mut rows = Vec::new();
    let (mut peak_rss, mut load_ms) = (0.0f64, 0u64);
    for (i, q) in qs.iter().enumerate() {
        let (t0, i0) = cpu_jiffies();
        let r = match ask(&[json!({"role": "user", "content": q})], 384, |_| {}) {
            Ok(r) => r,
            Err(e) => { eprintln!("{e}"); return 1 }
        };
        let (t1, i1) = cpu_jiffies();
        let cpu = if t1 > t0 { 100.0 * (1.0 - (i1 - i0) as f64 / (t1 - t0) as f64) } else { f64::NAN };
        let st = get_json("/v1/makeros/durum");
        let rss = st.pointer("/llama/rss_mb").and_then(Value::as_f64).unwrap_or(0.0);
        peak_rss = peak_rss.max(st.pointer("/llama/peak_rss_mb").and_then(Value::as_f64).unwrap_or(0.0));
        if i == 0 { load_ms = st.pointer("/llama/load_ms").and_then(Value::as_u64).unwrap_or(0); }
        let tm = &r.timings;
        let row = json!({
            "soru": q, "soguk": i == 0,
            "ilk_yanit_ms": (r.ttft_ms * 10.0).round() / 10.0, "toplam_ms": r.total_ms.round(),
            "istem_token": tm.get("prompt_n"), "istem_token_sn": tm.get("prompt_per_second"),
            "uretilen_token": tm.get("predicted_n"), "uretim_token_sn": tm.get("predicted_per_second"),
            "cpu_yuzde": (cpu * 10.0).round() / 10.0, "rss_mb": (rss * 10.0).round() / 10.0, "sicaklik_c": temp_c(),
            "komutlar": r.commands.iter().map(|a| json!({"komut": a.command, "risk": a.risk.label()})).collect::<Vec<_>>(),
        });
        if show && !json_out {
            println!("\n{} {q}\n{}", c.c("1;36", "›"), r.text.trim());
            print_commands(&r.commands);
        }
        rows.push(row);
    }
    let f = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN);
    let warm: Vec<&Value> = rows.iter().filter(|r| r["soguk"] == false).collect();
    let avg = |k: &str| { let v: Vec<f64> = warm.iter().map(|r| f(r, k)).filter(|x| x.is_finite()).collect(); if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 } };
    let summary = json!({
        "model": super::server::MODEL_ID,
        "model_yukleme_ms": load_ms,
        "soguk_ilk_yanit_ms": rows.first().map(|r| f(r, "ilk_yanit_ms")),
        "sicak_ilk_yanit_ms_ort": avg("ilk_yanit_ms"),
        "uretim_token_sn_ort": avg("uretim_token_sn"),
        "istem_token_sn_ort": avg("istem_token_sn"),
        "cpu_yuzde_ort": avg("cpu_yuzde"),
        "rss_mb_tepe": peak_rss,
        "bos_ram_mb_once": mem_before.round(),
        "sicaklik_c_son": temp_c(),
        "sorular": rows,
    });
    if json_out {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap_or_default());
        return 0;
    }
    println!("\n{}", c.c("1", "── Performans raporu (Raspberry Pi 4) ──"));
    println!("{:<44} {:>8} {:>8} {:>8} {:>7} {:>6}", "soru", "ilk(sn)", "tok/sn", "istem", "CPU%", "RSS");
    for r in &rows {
        let q: String = r["soru"].as_str().unwrap_or("").chars().take(42).collect();
        println!("{:<44} {:>8.1} {:>8.1} {:>8} {:>7.0} {:>6.0}{}", q, f(r, "ilk_yanit_ms") / 1000.0, f(r, "uretim_token_sn"), r["istem_token"].as_u64().unwrap_or(0), f(r, "cpu_yuzde"), f(r, "rss_mb"), if r["soguk"] == true { "  (soğuk)" } else { "" });
    }
    println!("\nModel yükleme: {:.1} sn · sıcak ilk yanıt ort.: {:.1} sn · üretim: {:.1} token/sn · istem işleme: {:.1} token/sn", load_ms as f64 / 1000.0, avg("ilk_yanit_ms") / 1000.0, avg("uretim_token_sn"), avg("istem_token_sn"));
    println!("RAM (llama-server tepe RSS): {:.0} MB · ölçüm öncesi boş RAM: {:.0} MB · ortalama CPU: %{:.0} · sıcaklık: {:.1} °C", peak_rss, mem_before, avg("cpu_yuzde"), temp_c());
    0
}
