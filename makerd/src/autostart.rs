//! Otomatik başlatma ve zamanlanmış görevler (Cron / @reboot):
//! Kullanıcının Python projelerini (`~/Projeler/<ad>`) veya özel betiklerini/komutlarını
//! sistem açılışında (`@reboot`) ya da belirli zamanlarda (5 alanlı cron ifadesi) çalıştırır.
//! Görevler kullanıcının ev dizininde `~/.config/makeros/tasks.json` dosyasında saklanır.

use crate::apierr::{ApiError, ApiResult};
use crate::pyman;
use crate::uagent::{self, Agent};
use serde::{Deserialize, Serialize};

pub const TASKS_PATH: &str = ".config/makeros/tasks.json";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Task {
    pub id: String,
    pub name: String,
    /// "project" (~/Projeler/<target>) veya "script" (kabuk/python komutu ya da betik yolu)
    pub kind: String,
    /// Proje adı ya da çalıştırılacak betik/komut
    pub target: String,
    /// "@reboot" veya 5 alanlı cron ("*/5 * * * *", "30 8 * * *" vb.)
    pub schedule: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_code: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_output: String,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Default)]
pub struct CreateReq {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// "project" | "script"
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub schedule: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Görev listesini okur.
pub async fn list(a: &dyn Agent, user: &str) -> Vec<Task> {
    let Ok(raw) = uagent::read_all(a, user, TASKS_PATH).await else { return Vec::new() };
    serde_json::from_slice(&raw).unwrap_or_default()
}

async fn save_list(a: &dyn Agent, user: &str, tasks: &[Task]) -> ApiResult<()> {
    let mut b = serde_json::to_vec_pretty(tasks)?;
    b.push(b'\n');
    uagent::write_all(a, user, TASKS_PATH, b).await
}

/// Cron veya @reboot zamanlamasını doğrular.
pub fn valid_schedule(s: &str) -> ApiResult<String> {
    let s = s.trim();
    if matches!(s, "@reboot" | "@boot") {
        return Ok("@reboot".into());
    }
    if s == "@hourly" {
        return Ok("0 * * * *".into());
    }
    if s == "@daily" {
        return Ok("0 0 * * *".into());
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(ApiError::invalid("Zamanlama '@reboot' veya 5 alanlı cron biçiminde olmalı (ör. '*/5 * * * *' veya '30 08 * * *')."));
    }
    let bounds = [(0u32, 59u32), (0, 23), (1, 31), (1, 12), (0, 7)];
    for (p, (min, max)) in parts.iter().zip(bounds) {
        if !valid_field(p, min, max) {
            return Err(ApiError::invalid(format!("Geçersiz cron alanı: '{p}' ({min}..={max})")));
        }
    }
    Ok(parts.join(" "))
}

fn valid_field(f: &str, min: u32, max: u32) -> bool {
    if f.is_empty() {
        return false;
    }
    for item in f.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((r, st)) => {
                let Ok(n) = st.parse::<u32>() else { return false };
                if n == 0 {
                    return false;
                }
                (r, Some(n))
            }
            None => (item, None),
        };
        if range == "*" {
            continue;
        }
        if let Some((a, b)) = range.split_once('-') {
            let (Ok(va), Ok(vb)) = (a.parse::<u32>(), b.parse::<u32>()) else { return false };
            if va < min || vb > max || va > vb {
                return false;
            }
        } else {
            if step.is_some() {
                return false;
            }
            let Ok(v) = range.parse::<u32>() else { return false };
            if v < min || v > max {
                return false;
            }
        }
    }
    true
}

/// Verilen (dakika, saat, gün, ay, haftanın_günü 0=Pazar..6=Cumartesi) cron ifadesiyle eşleşiyor mu?
pub fn matches_cron(expr: &str, min: u32, hour: u32, dom: u32, month: u32, dow: u32) -> bool {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }
    field_matches(parts[0], min, 0, 59)
        && field_matches(parts[1], hour, 0, 23)
        && field_matches(parts[2], dom, 1, 31)
        && field_matches(parts[3], month, 1, 12)
        && (field_matches(parts[4], dow, 0, 7) || (dow == 0 && field_matches(parts[4], 7, 0, 7)))
}

fn field_matches(f: &str, val: u32, min: u32, _max: u32) -> bool {
    for item in f.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((r, st)) => (r, st.parse::<u32>().unwrap_or(1).max(1)),
            None => (item, 1),
        };
        if range == "*" {
            if (val - min) % step == 0 {
                return true;
            }
        } else if let Some((a, b)) = range.split_once('-') {
            if let (Ok(va), Ok(vb)) = (a.parse::<u32>(), b.parse::<u32>()) {
                if val >= va && val <= vb && (val - va) % step == 0 {
                    return true;
                }
            }
        } else if let Ok(v) = range.parse::<u32>() {
            if v == val {
                return true;
            }
        }
    }
    false
}

/// Görev ekler veya günceller.
pub async fn upsert(a: &dyn Agent, user: &str, req: CreateReq) -> ApiResult<Task> {
    let kind = match req.kind.as_str() {
        "project" => "project",
        "script" | "" => "script",
        _ => return Err(ApiError::invalid("Görev türü 'project' veya 'script' olmalı.")),
    };
    let target = req.target.trim().to_string();
    if target.is_empty() || target.len() > 1000 || target.contains('\0') {
        return Err(ApiError::invalid("Hedef proje veya betik komutu gerekli."));
    }
    if kind == "project" {
        pyman::valid_name(&target)?;
    }
    let schedule = valid_schedule(&req.schedule)?;
    let name = if req.name.trim().is_empty() { target.clone() } else { req.name.trim().to_string() };
    let mut tasks = list(a, user).await;
    let id = if req.id.trim().is_empty() { crate::util::random_hex(6) } else { req.id.trim().to_string() };
    let task = if let Some(existing) = tasks.iter_mut().find(|t| t.id == id) {
        existing.name = name;
        existing.kind = kind.into();
        existing.target = target;
        existing.schedule = schedule;
        existing.enabled = req.enabled;
        existing.clone()
    } else {
        let t = Task {
            id,
            name,
            kind: kind.into(),
            target,
            schedule,
            enabled: req.enabled,
            last_run: None,
            last_code: None,
            last_output: String::new(),
        };
        tasks.push(t.clone());
        t
    };
    save_list(a, user, &tasks).await?;
    Ok(task)
}

/// Görevin etkin durumunu değiştirir.
pub async fn set_enabled(a: &dyn Agent, user: &str, id: &str, enabled: bool) -> ApiResult<Task> {
    let mut tasks = list(a, user).await;
    let t = tasks.iter_mut().find(|t| t.id == id).ok_or_else(|| ApiError::not_found("Görev bulunamadı."))?;
    t.enabled = enabled;
    let out = t.clone();
    save_list(a, user, &tasks).await?;
    Ok(out)
}

/// Görevi siler.
pub async fn remove(a: &dyn Agent, user: &str, id: &str) -> ApiResult<()> {
    let mut tasks = list(a, user).await;
    let before = tasks.len();
    tasks.retain(|t| t.id != id);
    if tasks.len() == before {
        return Err(ApiError::not_found("Görev bulunamadı."));
    }
    save_list(a, user, &tasks).await
}

/// Görevi çalıştıracak kabuk komutunu üretir.
async fn build_cmd(a: &dyn Agent, user: &str, task: &Task) -> ApiResult<String> {
    if task.kind == "project" {
        pyman::start_command(a, user, &task.target).await
    } else {
        let t = task.target.trim();
        // Eğer doğrudan bir .py dosyası yazıldıysa python3 ile, .sh ise sh ile, değilse kabuk komutu olarak çalıştır.
        let inner = if t.ends_with(".py") && !t.contains(' ') {
            format!("python3 {t}")
        } else if t.ends_with(".sh") && !t.contains(' ') {
            format!("sh {t}")
        } else {
            t.to_string()
        };
        Ok(format!("cd ~ && {inner}"))
    }
}

/// Görevi hemen çalıştırır ve sonucunu kaydeder.
pub async fn run_now(a: &dyn Agent, user: &str, id: &str) -> ApiResult<Task> {
    let mut tasks = list(a, user).await;
    let idx = tasks.iter().position(|t| t.id == id).ok_or_else(|| ApiError::not_found("Görev bulunamadı."))?;
    let task = tasks[idx].clone();
    let updated = exec_task(a, user, task).await?;
    tasks[idx] = updated.clone();
    save_list(a, user, &tasks).await?;
    Ok(updated)
}

async fn exec_task(a: &dyn Agent, user: &str, mut task: Task) -> ApiResult<Task> {
    let cmd = build_cmd(a, user, &task).await?;
    let wrapped = crate::ai::exec::wrap(&cmd);
    let (raw, _) = uagent::output(a, user, &wrapped).await?;
    let (out, code, _) = crate::ai::exec::parse_output(&raw);
    let now = jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S").to_string();
    task.last_run = Some(now);
    task.last_code = code.or(Some(0));
    task.last_output = out.chars().take(1000).collect();
    Ok(task)
}

/// Sistem açılışında `@reboot` görevlerini çalıştırır.
pub async fn run_boot_tasks(a: &dyn Agent, user: &str) {
    let mut tasks = list(a, user).await;
    let mut changed = false;
    for t in tasks.iter_mut() {
        if t.enabled && t.schedule == "@reboot" {
            if let Ok(u) = exec_task(a, user, t.clone()).await {
                *t = u;
                changed = true;
                tracing::info!(user, task = %t.name, "açılış görevi çalıştırıldı");
            }
        }
    }
    if changed {
        let _ = save_list(a, user, &tasks).await;
    }
}

/// Dakikalık zamanlayıcı: zamanı gelen etkin cron görevlerini çalıştırır.
pub async fn tick_cron_tasks(a: &dyn Agent, user: &str) {
    let now = jiff::Zoned::now();
    let min = now.minute() as u32;
    let hour = now.hour() as u32;
    let dom = now.day() as u32;
    let month = now.month() as u32;
    let dow = now.weekday().to_sunday_zero_offset() as u32;
    let stamp_prefix = now.strftime("%Y-%m-%d %H:%M").to_string();

    let mut tasks = list(a, user).await;
    let mut changed = false;
    for t in tasks.iter_mut() {
        if !t.enabled || t.schedule == "@reboot" {
            continue;
        }
        if t.last_run.as_deref().is_some_and(|s| s.starts_with(&stamp_prefix)) {
            continue;
        }
        if matches_cron(&t.schedule, min, hour, dom, month, dow) {
            if let Ok(u) = exec_task(a, user, t.clone()).await {
                *t = u;
                changed = true;
                tracing::info!(user, task = %t.name, schedule = %t.schedule, "zamanlanmış görev çalıştırıldı");
            }
        }
    }
    if changed {
        let _ = save_list(a, user, &tasks).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_validation_and_matching() {
        assert_eq!(valid_schedule("@reboot").unwrap(), "@reboot");
        assert_eq!(valid_schedule("*/5 * * * *").unwrap(), "*/5 * * * *");
        assert_eq!(valid_schedule("30 8 * * 1-5").unwrap(), "30 8 * * 1-5");
        assert!(valid_schedule("invalid").is_err());
        assert!(valid_schedule("60 * * * *").is_err());

        assert!(matches_cron("*/5 * * * *", 15, 10, 8, 11, 0));
        assert!(!matches_cron("*/5 * * * *", 14, 10, 8, 11, 0));
        assert!(matches_cron("30 8 * * 0", 30, 8, 8, 11, 0));
        assert!(matches_cron("30 8 * * 7", 30, 8, 8, 11, 0));
        assert!(!matches_cron("30 8 * * 1-5", 30, 8, 8, 11, 0));
    }
}
