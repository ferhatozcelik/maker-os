//! makeros-ai: Raspberry Pi 4'te internetsiz çalışan Türkçe Linux asistanı (Qwen2.5-0.5B-Instruct Q4_K_M + llama.cpp).
//!
//! - `llama`: llama-server sürecini istek gelince başlatır, boşta kapatır.
//! - `server`: OpenAI uyumlu yerel API (sistem istemi, geçmiş, bağlam yönetimi, akış).
//! - `prompt`: Türkçe sistem istemi ve doğrulanmış komut kartları.
//! - `risk`: komutların kurallı risk sınıflandırması (SAFE / WARNING / DANGEROUS).
//! - `cli`: terminal istemcisi ve performans ölçümü.
//!
//! İlk aşamada hiçbir komut çalıştırılmaz: yalnız önerilir ve risk düzeyi gösterilir.

pub mod cli;
pub mod exec;
pub mod llama;
pub mod prompt;
pub mod risk;
pub mod server;
