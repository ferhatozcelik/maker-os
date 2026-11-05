//! Raspberry Pi'nin 40 pinlik başlığı (Pi 4, Pi 400, Pi 5 aynı yerleşim).
//! Gerçek sistemde GPIO karakter aygıtı (/dev/gpiochipN) kullanılır: Pi 4'te
//! "pinctrl-bcm2711", Pi 5'te RP1 ("pinctrl-rp1"). Donanım yoksa (Docker
//! önizleme) benzetim kullanılır.

use crate::apierr::{ApiError, ApiResult};
use gpiocdev::line::{Direction, Value};
use gpiocdev::{Chip, Request};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

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
}

impl Real {
    /// Başlığı süren yonga varsa (Pi 4: bcm2711/bcm2835, Pi 5: rp1) arka ucu döner.
    pub fn detect(model: &str) -> Option<Self> {
        for path in gpiocdev::chip::chips().ok()? {
            let Ok(info) = Chip::from_path(&path).and_then(|c| c.info()) else { continue };
            if ["bcm2711", "bcm2835", "rp1"].iter().any(|k| info.label.contains(k)) {
                return Some(Self { model: model.into(), chip: path, label: info.label, held: Mutex::new(HashMap::new()) });
            }
        }
        None
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
                p.value = r.value(off).map(val).unwrap_or(0);
                continue;
            }
            let Ok(info) = c.line_info(off) else { continue };
            p.used = info.used;
            p.consumer = info.consumer;
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
        Ok(())
    }
}

// --- benzetim ----------------------------------------------------------------

/// Donanımsız önizleme: GPIO4 her saniye değişen bir sensör gibi davranır,
/// GPIO2/3 (I2C) ve 14/15 (UART) çekirdek kullanımında görünür.
pub struct Sim {
    pub model: String,
    out: Mutex<HashMap<i32, i32>>,
    start: Instant,
}

impl Sim {
    pub fn new(model: &str) -> Self {
        Self { model: model.into(), out: Mutex::new(HashMap::new()), start: Instant::now() }
    }
}

impl Provider for Sim {
    fn board(&self) -> ApiResult<Board> {
        let out = self.out.lock().unwrap();
        let tick = self.start.elapsed().as_secs() as i32;
        let mut pins = header();
        for p in pins.iter_mut().filter(|p| p.gpio >= 0) {
            p.dir = "in".into();
            p.value = 0;
            match p.gpio {
                2 | 3 => {
                    p.used = true;
                    p.consumer = "i2c1".into();
                    p.value = 1;
                }
                4 => p.value = tick % 2,
                14 | 15 => {
                    p.used = true;
                    p.consumer = "serial0".into();
                }
                _ => {}
            }
            if let Some(&v) = out.get(&p.gpio) {
                p.dir = "out".into();
                p.value = v;
                p.held = true;
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
}
