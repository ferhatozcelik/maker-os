//! Süreç listesi (/proc ayrıştırma). CPU yüzdesi iki örnek arasındaki tik
//! farkından (tek çekirdek = %100, top gibi) hesaplanır; cgroup dilimi
//! /proc/<pid>/cgroup'tan okunur.

use crate::apierr::{ApiError, ApiResult};
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// GET /processes öğesi.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Process {
    pub pid: i32,
    pub name: String,
    pub user: String,
    pub cpu_pct: f64,
    pub rss: i64,
    pub slice: String,
}

/// Süreç arka ucu (gerçek /proc veya mock).
pub trait Source: Send + Sync {
    fn processes(&self) -> BoxFuture<'_, ApiResult<Vec<Process>>>;
    fn signal<'a>(&'a self, pid: i32, sig: &'a str) -> BoxFuture<'a, ApiResult<()>>;
}

/// Dönen en fazla süreç sayısı.
pub const MAX_LIST: usize = 100;
/// Linux arm64/amd64 USER_HZ.
const CLK_TCK: f64 = 100.0;

/// /proc/<pid>/stat'ın gerekli alanları.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub pid: i32,
    pub comm: String,
    /// utime + stime
    pub ticks: u64,
    /// starttime
    pub start: u64,
    /// sayfa
    pub rss: i64,
}

/// Bir /proc/<pid>/stat satırını ayrıştırır. comm boşluk ve ')' içerebilir.
pub fn parse_stat(s: &str) -> Option<Stat> {
    let open = s.find('(')?;
    let end = s.rfind(')')?;
    if end < open || end + 2 > s.len() {
        return None;
    }
    let pid = s[..open].trim().parse().ok()?;
    let f: Vec<&str> = s[end + 2..].split_whitespace().collect(); // f[0] = alan 3 (state)
    if f.len() < 22 {
        return None;
    }
    let ut: u64 = f[11].parse().unwrap_or(0);
    let st: u64 = f[12].parse().unwrap_or(0);
    let start = f[19].parse().unwrap_or(0);
    let rss: i64 = f[21].parse().unwrap_or(0);
    Some(Stat { pid, comm: s[open + 1..end].to_string(), ticks: ut + st, start, rss: rss.max(0) })
}

/// cgroup v2 yolundaki en derin ".slice" bileşeni.
/// "0::/user.slice/user-1000.slice/session-2.scope" → "user-1000.slice".
pub fn slice_of(cgroup: &str) -> String {
    let path = cgroup.lines().find_map(|l| l.strip_prefix("0::")).unwrap_or("");
    path.split('/').filter(|p| p.ends_with(".slice")).last().unwrap_or("").to_string()
}

/// /proc/<pid>/status içindeki gerçek UID.
pub fn uid_of(status: &str) -> Option<u32> {
    status.lines().find_map(|l| l.strip_prefix("Uid:")).and_then(|v| v.split_whitespace().next()).and_then(|f| f.parse().ok())
}

#[derive(Clone, Default)]
struct Meta {
    start: u64,
    uid: u32,
    slice: String,
}

#[derive(Clone)]
struct Sample {
    at: Instant,
    /// pid → (ticks, start)
    ticks: HashMap<i32, (u64, u64)>,
    list: Vec<Process>,
}

#[derive(Default)]
struct State {
    last: Option<Sample>,
    meta: HashMap<i32, Meta>,
    users: HashMap<u32, String>,
    users_at: Option<Instant>,
}

/// Gerçek /proc okuyucusu.
pub struct Proc {
    /// "/" (testte fikstür kökü)
    pub root: PathBuf,
    pub passwd: String,
    pub page_size: i64,
    /// Sinyal yetkisi: yalnız bu UID'nin süreçleri.
    pub self_uid: u32,
    pub self_pid: i32,
    st: Mutex<State>,
}

impl Proc {
    pub fn new() -> Self {
        // SAFETY: sysconf yan etkisizdir.
        let ps = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        Self::with_root("/", "/etc/passwd", if ps > 0 { ps as i64 } else { 4096 }, crate::util::getuid(), std::process::id() as i32)
    }

    pub fn with_root(root: impl Into<PathBuf>, passwd: &str, page_size: i64, self_uid: u32, self_pid: i32) -> Self {
        Self { root: root.into(), passwd: passwd.into(), page_size, self_uid, self_pid, st: Mutex::new(State::default()) }
    }

    fn read(&self, pid: i32, file: &str) -> String {
        std::fs::read_to_string(self.root.join("proc").join(pid.to_string()).join(file)).unwrap_or_default()
    }

    fn user_name(&self, st: &mut State, uid: u32, now: Instant) -> String {
        if st.users_at.is_none_or(|t| now.duration_since(t) > Duration::from_secs(60)) {
            st.users = crate::util::passwd_entries(&self.passwd).into_iter().map(|e| (e.uid, e.name)).collect();
            st.users_at = Some(now);
        }
        st.users.get(&uid).cloned().unwrap_or_else(|| uid.to_string())
    }

    /// Tek örnek (önceki örneğe göre CPU yüzdesiyle).
    fn take(&self, st: &mut State, prev: Option<&Sample>, now: Instant) -> std::io::Result<Sample> {
        let rd = std::fs::read_dir(self.root.join("proc"))?;
        let dt = prev.map(|p| now.saturating_duration_since(p.at).as_secs_f64()).unwrap_or(0.0);
        let mut s = Sample { at: now, ticks: HashMap::new(), list: Vec::new() };
        let mut seen = std::collections::HashSet::new();
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else { continue };
            // Okunurken sonlanan süreç atlanır.
            let Some(stat) = parse_stat(&self.read(pid, "stat")) else { continue };
            seen.insert(pid);
            let m = match st.meta.get(&pid) {
                Some(m) if m.start == stat.start => m.clone(),
                _ => {
                    let m = Meta { start: stat.start, uid: uid_of(&self.read(pid, "status")).unwrap_or(0), slice: slice_of(&self.read(pid, "cgroup")) };
                    st.meta.insert(pid, m.clone());
                    m
                }
            };
            let mut p = Process { pid, name: stat.comm, user: self.user_name(st, m.uid, now), cpu_pct: 0.0, rss: stat.rss * self.page_size, slice: m.slice };
            if let Some(prev) = prev.filter(|_| dt > 0.0) {
                if let Some(&(t, start)) = prev.ticks.get(&pid) {
                    if start == stat.start && stat.ticks >= t {
                        p.cpu_pct = ((stat.ticks - t) as f64 / CLK_TCK / dt * 1000.0).round() / 10.0;
                    }
                }
            }
            s.ticks.insert(pid, (stat.ticks, stat.start));
            s.list.push(p);
        }
        st.meta.retain(|pid, _| seen.contains(pid));
        s.list.sort_by(|a, b| {
            b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal).then(b.rss.cmp(&a.rss)).then(a.pid.cmp(&b.pid))
        });
        Ok(s)
    }

    /// En çok CPU kullanan ilk 100 süreç. Son örnek 1 sn'den yeniyse önbellekten
    /// verilir; önceki örnek yoksa (veya çok eskiyse) kısa aralıkla iki örnek alınır.
    pub async fn list(&self) -> ApiResult<Vec<Process>> {
        let mut st = self.st.lock().await;
        let now = Instant::now();
        if let Some(l) = st.last.as_ref().filter(|l| now.duration_since(l.at) < Duration::from_secs(1)) {
            return Ok(limit(&l.list));
        }
        let mut prev = st.last.clone().filter(|p| now.duration_since(p.at) <= Duration::from_secs(30));
        if prev.is_none() {
            prev = Some(self.take(&mut st, None, now).map_err(|e| ApiError::internal(e.to_string()))?);
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let s = self.take(&mut st, prev.as_ref(), Instant::now()).map_err(|e| ApiError::internal(e.to_string()))?;
        let out = limit(&s.list);
        st.last = Some(s);
        Ok(out)
    }

    /// Yalnız makerd ile aynı kullanıcıya ait süreçlere sinyal gönderir.
    pub fn send_signal(&self, pid: i32, sig: &str) -> ApiResult<()> {
        let s = parse_signal(sig).ok_or_else(|| ApiError::invalid("Sinyal TERM veya KILL olmalıdır."))?;
        if pid <= 1 || pid == self.self_pid {
            return Err(ApiError::forbidden("Bu sürece sinyal gönderilemez."));
        }
        let uid = uid_of(&self.read(pid, "status")).ok_or_else(|| ApiError::not_found("Süreç bulunamadı."))?;
        if uid != self.self_uid {
            return Err(ApiError::forbidden("Yalnızca kendi süreçlerinizi sonlandırabilirsiniz."));
        }
        match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), s) {
            Ok(()) => Ok(()),
            Err(nix::errno::Errno::ESRCH) => Err(ApiError::not_found("Süreç bulunamadı.")),
            Err(e) => Err(ApiError::forbidden(format!("Sinyal gönderilemedi: {}", e.desc()))),
        }
    }
}

impl Default for Proc {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for Proc {
    fn processes(&self) -> BoxFuture<'_, ApiResult<Vec<Process>>> {
        Box::pin(self.list())
    }
    fn signal<'a>(&'a self, pid: i32, sig: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move { self.send_signal(pid, sig) })
    }
}

fn limit(l: &[Process]) -> Vec<Process> {
    l.iter().take(MAX_LIST).cloned().collect()
}

/// Sözleşmedeki sinyal adı.
pub fn parse_signal(s: &str) -> Option<nix::sys::signal::Signal> {
    match s {
        "TERM" => Some(nix::sys::signal::Signal::SIGTERM),
        "KILL" => Some(nix::sys::signal::Signal::SIGKILL),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn stat_parsing() {
        let s = parse_stat("1 (systemd) S 0 1 1 0 -1 4194560 100 0 10 0 50 30 0 0 20 0 1 0 5 170000000 2800 0").unwrap();
        assert_eq!((s.pid, s.comm.as_str(), s.ticks, s.start, s.rss), (1, "systemd", 80, 5, 2800));
        let s = parse_stat("1240 (Web Content) (x)) R 1 1240 1240 0 -1 0 0 0 0 0 1000 500 0 0 20 0 4 0 5000 900000000 16384 0").unwrap();
        assert_eq!((s.pid, s.comm.as_str(), s.ticks, s.start, s.rss), (1240, "Web Content) (x)", 1500, 5000, 16384));
        assert!(parse_stat("garbage").is_none());
        assert!(parse_stat("5 (short) S 1 2 3").is_none());
    }

    #[test]
    fn slice_and_uid() {
        for (i, w) in [
            ("0::/user.slice/user-1000.slice/session-2.scope", "user-1000.slice"),
            ("0::/system.slice/makerd.service", "system.slice"),
            ("0::/apps.slice/app-foo.scope", "apps.slice"),
            ("0::/", ""),
            ("12:cpu:/x\n0::/shell.slice/cog.service\n", "shell.slice"),
        ] {
            assert_eq!(slice_of(i), w, "{i}");
        }
        assert_eq!(uid_of("Name:\tx\nUid:\t1000\t1000\t1000\t1000\n"), Some(1000));
        assert_eq!(uid_of("Name: x\n"), None);
    }

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("mk-act-{}", crate::util::random_hex(4)));
        copy_tree(Path::new("testdata/activity/proc"), &root.join("proc"));
        root
    }

    fn copy_tree(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for e in std::fs::read_dir(src).unwrap().flatten() {
            let t = dst.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                copy_tree(&e.path(), &t);
            } else {
                std::fs::copy(e.path(), t).unwrap();
            }
        }
    }

    #[test]
    fn cpu_delta() {
        let root = fixture();
        let p = Proc::with_root(&root, "testdata/activity/passwd", 4096, 1000, 0);
        let mut st = State::default();
        let t0 = Instant::now();
        let first = p.take(&mut st, None, t0).unwrap();
        assert_eq!(first.list.len(), 4);
        // 2 sn sonra makerd 100 tik (1 sn CPU) daha kullanmış: %50.
        let stat = root.join("proc/431/stat");
        let b = std::fs::read_to_string(&stat).unwrap();
        std::fs::write(&stat, b.replacen(" 200 100 ", " 260 140 ", 1)).unwrap();
        let second = p.take(&mut st, Some(&first), t0 + Duration::from_secs(2)).unwrap();
        let top = &second.list[0];
        assert_eq!((top.pid, top.cpu_pct, top.user.as_str(), top.slice.as_str(), top.rss), (431, 50.0, "maker", "system.slice", 3584 * 4096));
        assert!(second.list[1..].iter().all(|p| p.cpu_pct == 0.0));
        // Ölen süreç meta'dan temizlenir.
        std::fs::remove_dir_all(root.join("proc/77")).unwrap();
        let third = p.take(&mut st, Some(&second), t0 + Duration::from_secs(3)).unwrap();
        assert_eq!(third.list.len(), 3);
        assert!(!st.meta.contains_key(&77));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cached_list() {
        let root = fixture();
        let p = Proc::with_root(&root, "testdata/activity/passwd", 4096, 1000, 0);
        let a = p.list().await.unwrap();
        // 1 sn içinde ikinci çağrı önbellekten (dosya değişse de aynı liste).
        std::fs::remove_dir_all(root.join("proc/77")).unwrap();
        assert_eq!(p.list().await.unwrap(), a);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn signal_rules() {
        let root = fixture();
        let p = Proc::with_root(&root, "", 4096, 1000, 431);
        for (pid, sig, code) in [(431, "TERM", "forbidden"), (1, "TERM", "forbidden"), (77, "KILL", "forbidden"), (1240, "HUP", "invalid"), (99999, "TERM", "not_found")] {
            assert_eq!(p.send_signal(pid, sig).unwrap_err().code, code, "{pid} {sig}");
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
