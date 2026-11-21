//! Raspberry Pi'nin 40 pinlik başlığı (Pi 4, Pi 400, Pi 5 aynı yerleşim).
//! Gerçek sistemde GPIO karakter aygıtı (/dev/gpiochipN) kullanılır: Pi 4'te
//! "pinctrl-bcm2711", Pi 5'te RP1 ("pinctrl-rp1"). Donanım yoksa (Docker
//! önizleme) benzetim kullanılır.

use crate::apierr::{ApiError, ApiResult};
use gpiocdev::line::{Bias, Direction, Value};
use gpiocdev::{Chip, Request};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Başlıktaki fiziksel pin.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub phys: i32,
    /// "GPIO17", "3V3", "GND", "5V"
    pub name: String,
    /// -1: güç/toprak
    pub gpio: i32,
    /// "I2C1 SDA", "SPI0 MOSI", "UART TX" ...
    #[serde(skip_serializing_if = "String::is_empty")]
    pub alt: String,
    /// "in" | "out"
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dir: String,
    /// 0/1 (güç pinlerinde -1)
    pub value: i32,
    /// Başka bir süreç veya çekirdek kullanıyor.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub used: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub consumer: String,
    /// MakerOS (bu panel) çıkış olarak tutuyor.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub held: bool,
    /// İç direnç: "up" | "down" | "off" (bilinmiyorsa boş).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bias: String,
    /// Bağlantı tahmini (son taramadan): "none" boşta, "high"/"low" dışarıdan 1/0'a sürülüyor,
    /// "signal" tarama sırasında değişti, "bus" çekirdek/başka program, "out" çıkış. Taranmadıysa boş.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub conn: String,
}

/// Panel yanıtı.
#[derive(Serialize, Clone, Debug)]
pub struct Board {
    pub model: String,
    pub chip: String,
    pub label: String,
    pub sim: bool,
    pub pins: Vec<Pin>,
}

/// Fiziksel pin → (ad, GPIO, alternatif işlev).
const HEADER: [(i32, &str, i32, &str); 40] = [
    (1, "3V3", -1, ""), (2, "5V", -1, ""),
    (3, "GPIO2", 2, "I2C1 SDA"), (4, "5V", -1, ""),
    (5, "GPIO3", 3, "I2C1 SCL"), (6, "GND", -1, ""),
    (7, "GPIO4", 4, "GPCLK0"), (8, "GPIO14", 14, "UART TX"),
    (9, "GND", -1, ""), (10, "GPIO15", 15, "UART RX"),
    (11, "GPIO17", 17, ""), (12, "GPIO18", 18, "PCM CLK / PWM0"),
    (13, "GPIO27", 27, ""), (14, "GND", -1, ""),
    (15, "GPIO22", 22, ""), (16, "GPIO23", 23, ""),
    (17, "3V3", -1, ""), (18, "GPIO24", 24, ""),
    (19, "GPIO10", 10, "SPI0 MOSI"), (20, "GND", -1, ""),
    (21, "GPIO9", 9, "SPI0 MISO"), (22, "GPIO25", 25, ""),
    (23, "GPIO11", 11, "SPI0 SCLK"), (24, "GPIO8", 8, "SPI0 CE0"),
    (25, "GND", -1, ""), (26, "GPIO7", 7, "SPI0 CE1"),
    (27, "GPIO0", 0, "ID EEPROM SDA"), (28, "GPIO1", 1, "ID EEPROM SCL"),
    (29, "GPIO5", 5, ""), (30, "GND", -1, ""),
    (31, "GPIO6", 6, ""), (32, "GPIO12", 12, "PWM0"),
    (33, "GPIO13", 13, "PWM1"), (34, "GND", -1, ""),
    (35, "GPIO19", 19, "PCM FS / SPI1 MISO"), (36, "GPIO16", 16, "SPI1 CE2"),
    (37, "GPIO26", 26, ""), (38, "GPIO20", 20, "PCM DIN / SPI1 MOSI"),
    (39, "GND", -1, ""), (40, "GPIO21", 21, "PCM DOUT / SPI1 SCLK"),
];

/// Başlık yerleşimi (yeni kopya).
pub fn header() -> Vec<Pin> {
    HEADER
        .iter()
        .map(|&(phys, name, gpio, alt)| Pin {
            phys,
            name: name.into(),
            gpio,
            alt: alt.into(),
            dir: String::new(),
            value: if gpio < 0 { -1 } else { 0 },
            used: false,
            consumer: String::new(),
            held: false,
            bias: String::new(),
            conn: String::new(),
        })
        .collect()
}

/// Panel arka ucu.
pub trait Provider: Send + Sync {
    fn board(&self) -> ApiResult<Board>;
    /// Pini çıkış yapıp değeri yazar ve tutar (release edilene kadar).
    fn set(&self, gpio: i32, value: i32) -> ApiResult<()>;
    /// Tutulan pini bırakır (giriş olarak serbest).
    fn release(&self, gpio: i32) -> ApiResult<()>;
    /// Bağlantı taraması: boştaki giriş pinine sırayla iç pull-up ve pull-down uygulanıp okunur. Değer dirence
    /// uyuyorsa pin boşta (bir şey bağlı değil); sabit kalıyorsa dışarıdan sürülüyor (bağlı bir şey var).
    /// `None`: tüm boş pinler. Sonuç `Pin::conn` alanında görünür.
    fn probe(&self, gpio: Option<i32>) -> ApiResult<()>;
}

fn conn_of(up: i32, down: i32) -> &'static str {
    match (up, down) {
        (1, 0) => "none",
        (1, 1) => "high",
        (0, 0) => "low",
        _ => "signal",
    }
}

fn bias_name(b: Option<Bias>) -> &'static str {
    match b {
        Some(Bias::PullUp) => "up",
        Some(Bias::PullDown) => "down",
        Some(Bias::Disabled) => "off",
        None => "",
    }
}

const CONSUMER: &str = "makeros-panel";

fn valid(gpio: i32) -> ApiResult<()> {
    if gpio >= 0 && HEADER.iter().any(|p| p.2 == gpio) {
        if gpio <= 1 {
            return Err(ApiError::invalid("GPIO0/1 HAT kimlik EEPROM'u içindir; kullanılmaz."));
        }
        return Ok(());
    }
    Err(ApiError::invalid("Başlıkta böyle bir GPIO yok."))
}

fn val(v: Value) -> i32 {
    if v == Value::Active { 1 } else { 0 }
}

// --- gerçek donanım --------------------------------------------------------

/// Karakter aygıtı arka ucu.
pub struct Real {
    pub model: String,
    chip: PathBuf,
    label: String,
    held: Mutex<HashMap<i32, Request>>,
    conn: Mutex<HashMap<i32, &'static str>>,
}

impl Real {
    /// Başlığı süren yonga varsa (Pi 4: bcm2711/bcm2835, Pi 5: rp1) arka ucu döner.
    pub fn detect(model: &str) -> Option<Self> {
        for path in gpiocdev::chip::chips().ok()? {
            let Ok(info) = Chip::from_path(&path).and_then(|c| c.info()) else { continue };
            if ["bcm2711", "bcm2835", "rp1"].iter().any(|k| info.label.contains(k)) {
                return Some(Self { model: model.into(), chip: path, label: info.label, held: Mutex::new(HashMap::new()), conn: Mutex::new(HashMap::new()) });
            }
        }
        None
    }

    /// Tek hattı tarar (yalnız boştaki girişler). Tarama sonrası özgün iç direnç geri yüklenir.
    fn probe_line(&self, off: u32) -> Option<&'static str> {
        let c = Chip::from_path(&self.chip).ok()?;
        let info = c.line_info(off).ok()?;
        if info.used || info.direction != Direction::Input {
            return None;
        }
        let read = |b: Bias| -> Option<i32> {
            let r = Request::builder().on_chip(&self.chip).with_line(off).as_input().with_bias(b).with_consumer(CONSUMER).request().ok()?;
            std::thread::sleep(Duration::from_millis(3)); // iç direncin (~50 kΩ) hattı çekmesi için
            r.value(off).map(val).ok()
        };
        let up = read(Bias::PullUp)?;
        let down = read(Bias::PullDown)?;
        if let Some(b) = info.bias {
            let _ = Request::builder().on_chip(&self.chip).with_line(off).as_input().with_bias(b).with_consumer(CONSUMER).request();
        }
        Some(conn_of(up, down))
    }

    fn chip_name(&self) -> String {
        self.chip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

impl Provider for Real {
    fn board(&self) -> ApiResult<Board> {
        let c = Chip::from_path(&self.chip).map_err(|e| ApiError::unavailable(format!("GPIO yongası açılamadı: {e}")))?;
        let held = self.held.lock().unwrap();
        let mut pins = header();
        for p in pins.iter_mut().filter(|p| p.gpio >= 0) {
            let off = p.gpio as u32;
            if let Some(r) = held.get(&p.gpio) {
                p.held = true;
                p.dir = "out".into();
                p.conn = "out".into();
                p.value = r.value(off).map(val).unwrap_or(0);
                continue;
            }
            let Ok(info) = c.line_info(off) else { continue };
            p.used = info.used;
            p.consumer = info.consumer;
            p.bias = bias_name(info.bias).into();
            p.conn = if info.used { "bus".into() } else if info.direction == Direction::Output { "out".into() } else { self.conn.lock().unwrap().get(&p.gpio).copied().unwrap_or_default().into() };
            p.dir = if info.direction == Direction::Output { "out" } else { "in" }.into();
            if !info.used {
                // Boştaki hat yönü değiştirilmeden kısa süre istenip okunur.
                if let Ok(r) = Request::builder().on_chip(&self.chip).with_line(off).as_is().with_consumer(CONSUMER).request() {
                    p.value = r.value(off).map(val).unwrap_or(0);
                }
            }
        }
        Ok(Board { model: self.model.clone(), chip: self.chip_name(), label: self.label.clone(), sim: false, pins })
    }

    fn set(&self, gpio: i32, value: i32) -> ApiResult<()> {
        valid(gpio)?;
        let v = if value != 0 { Value::Active } else { Value::Inactive };
        let off = gpio as u32;
        let mut held = self.held.lock().unwrap();
        if let Some(r) = held.get(&gpio) {
            return r.set_value(off, v).map_err(|e| ApiError::internal(e.to_string()));
        }
        if !crate::util::can_rw(&self.chip) {
            return Err(ApiError::forbidden("GPIO izni yok ('gpio' grubu)."));
        }
        let r = Request::builder()
            .on_chip(&self.chip)
            .with_line(off)
            .as_output(v)
            .with_consumer(CONSUMER)
            .request()
            .map_err(|_| ApiError::conflict(format!("GPIO{gpio} başka bir program tarafından kullanılıyor.")))?;
        held.insert(gpio, r);
        Ok(())
    }

    fn release(&self, gpio: i32) -> ApiResult<()> {
        // İstek düşürülünce hat serbest kalır.
        self.held.lock().unwrap().remove(&gpio);
        self.conn.lock().unwrap().remove(&gpio);
        Ok(())
    }

    fn probe(&self, gpio: Option<i32>) -> ApiResult<()> {
        if let Some(g) = gpio {
            valid(g)?;
        }
        let held: Vec<i32> = self.held.lock().unwrap().keys().copied().collect();
        let targets: Vec<i32> = HEADER.iter().map(|p| p.2).filter(|&g| g > 1 && gpio.is_none_or(|x| x == g) && !held.contains(&g)).collect();
        for g in targets {
            let r = self.probe_line(g as u32);
            let mut c = self.conn.lock().unwrap();
            match r {
                Some(v) => { c.insert(g, v); }
                None => { c.remove(&g); }
            }
        }
        Ok(())
    }
}

// --- benzetim ----------------------------------------------------------------

/// Donanımsız önizleme: GPIO4 her saniye değişen bir sensör gibi davranır,
/// GPIO2/3 (I2C) ve 14/15 (UART) çekirdek kullanımında görünür.
pub struct Sim {
    pub model: String,
    out: Mutex<HashMap<i32, i32>>,
    conn: Mutex<HashMap<i32, &'static str>>,
    start: Instant,
}

/// Benzetimdeki "bağlı" cihazlar: GPIO4 değişen sensör, GPIO27 dışarıdan 1'e çekili düğme, GPIO22 toprağa bağlı.
fn sim_conn(gpio: i32) -> &'static str {
    match gpio {
        4 => "signal",
        27 => "high",
        22 => "low",
        _ => "none",
    }
}

impl Sim {
    pub fn new(model: &str) -> Self {
        Self { model: model.into(), out: Mutex::new(HashMap::new()), conn: Mutex::new(HashMap::new()), start: Instant::now() }
    }
}

impl Provider for Sim {
    fn board(&self) -> ApiResult<Board> {
        let out = self.out.lock().unwrap();
        let conn = self.conn.lock().unwrap();
        let tick = self.start.elapsed().as_secs() as i32;
        let mut pins = header();
        for p in pins.iter_mut().filter(|p| p.gpio >= 0) {
            p.dir = "in".into();
            p.value = 0;
            p.bias = if p.gpio <= 8 { "up" } else { "down" }.into();
            p.conn = conn.get(&p.gpio).copied().unwrap_or_default().into();
            match p.gpio {
                2 | 3 => {
                    p.used = true;
                    p.consumer = "i2c1".into();
                    p.value = 1;
                    p.conn = "bus".into();
                }
                4 => p.value = tick % 2,
                27 => p.value = 1,
                14 | 15 => {
                    p.used = true;
                    p.consumer = "serial0".into();
                    p.conn = "bus".into();
                }
                _ => {}
            }
            if let Some(&v) = out.get(&p.gpio) {
                p.dir = "out".into();
                p.value = v;
                p.held = true;
                p.conn = "out".into();
            }
        }
        Ok(Board { model: self.model.clone(), chip: "sim".into(), label: "benzetim (donanım yok)".into(), sim: true, pins })
    }

    fn set(&self, gpio: i32, value: i32) -> ApiResult<()> {
        valid(gpio)?;
        self.out.lock().unwrap().insert(gpio, i32::from(value != 0));
        Ok(())
    }

    fn release(&self, gpio: i32) -> ApiResult<()> {
        self.out.lock().unwrap().remove(&gpio);
        Ok(())
    }

    fn probe(&self, gpio: Option<i32>) -> ApiResult<()> {
        if let Some(g) = gpio {
            valid(g)?;
        }
        let mut c = self.conn.lock().unwrap();
        for g in HEADER.iter().map(|p| p.2).filter(|&g| g > 1 && !matches!(g, 2 | 3 | 14 | 15) && gpio.is_none_or(|x| x == g)) {
            c.insert(g, sim_conn(g));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_layout() {
        let h = header();
        assert_eq!(h.len(), 40);
        assert!(h.iter().enumerate().all(|(i, p)| p.phys == i as i32 + 1));
        assert_eq!(h[10].name, "GPIO17");
        assert_eq!(h[0].value, -1);
        let j = serde_json::to_value(&h[0]).unwrap();
        assert_eq!(j, serde_json::json!({"phys":1,"name":"3V3","gpio":-1,"value":-1}));
    }

    #[test]
    fn sim_board() {
        let s = Sim::new("Raspberry Pi 4");
        assert_eq!(s.set(1, 1).unwrap_err().code, "invalid");
        assert_eq!(s.set(99, 1).unwrap_err().code, "invalid");
        assert_eq!(s.set(-1, 1).unwrap_err().code, "invalid");
        s.set(17, 5).unwrap();
        let b = s.board().unwrap();
        assert!(b.sim);
        let p = b.pins.iter().find(|p| p.gpio == 17).unwrap();
        assert_eq!((p.dir.as_str(), p.value, p.held), ("out", 1, true));
        let i2c = b.pins.iter().find(|p| p.gpio == 2).unwrap();
        assert_eq!((i2c.used, i2c.consumer.as_str()), (true, "i2c1"));
        s.release(17).unwrap();
        assert!(!s.board().unwrap().pins.iter().find(|p| p.gpio == 17).unwrap().held);
    }

    #[test]
    fn conn_logic_and_sim_probe() {
        assert_eq!((conn_of(1, 0), conn_of(1, 1), conn_of(0, 0), conn_of(0, 1)), ("none", "high", "low", "signal"));
        let s = Sim::new("Raspberry Pi 4");
        let pin = |b: &Board, g: i32| b.pins.iter().find(|p| p.gpio == g).unwrap().conn.clone();
        assert_eq!(pin(&s.board().unwrap(), 27), "", "taranmadan bilinmez");
        s.probe(Some(27)).unwrap();
        assert_eq!(pin(&s.board().unwrap(), 27), "high");
        s.probe(None).unwrap();
        let b = s.board().unwrap();
        assert_eq!((pin(&b, 4), pin(&b, 22), pin(&b, 17), pin(&b, 2)), ("signal".into(), "low".into(), "none".into(), "bus".into()));
        assert_eq!(s.probe(Some(1)).unwrap_err().code, "invalid");
    }
}
