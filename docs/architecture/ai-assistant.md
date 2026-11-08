# Yerel AI Asistanı

[Türkçe](ai-assistant.md) · [English](ai-assistant.en.md)

MakerOS, internete bağımlı olmadan çalışan yerel bir yapay zekâ asistanı ([`makerd/src/ai/`](../../makerd/src/ai/) ve [`shell/apps/ai-widget/`](../../shell/apps/ai-widget/)) içerir. Çıkarım altyapısında **Qwen2.5-0.5B-Instruct (GGUF Q4_K_M)** modeli ve `llama.cpp` (`llama-server`) kullanılır.

## Çalışma Modları ve Özellikler

- **Doğal Sohbet ve MakerOS Bilgi Bankası ([`prompt.rs`](../../makerd/src/ai/prompt.rs)):** Kullanıcı MakerOS uygulamaları, Python, elektronik veya genel konular hakkında soru sorduğunda zorunlu komut şablonuna girmeden doğal Türkçe ile yanıt verir.
- **Komut Önerisi ve Risk Sınıflandırması ([`risk.rs`](../../makerd/src/ai/risk.rs)):** Sistem ve terminal isteklerinde önerilen komutlar `SAFE` (salt okunur), `WARNING` (sistem değişikliği) ve `DANGEROUS` (yıkıcı işlem) olarak sınıflandırılır.
- **Çalıştırma Modları ([`exec.rs`](../../makerd/src/ai/exec.rs)):** Masaüstü widget'ından **Full Bypass** (doğrudan çalıştır) veya **İzin İste** (onayla çalıştır) modu seçilebilir; işlem süresince ilerleme çubuğu gösterilir ve **Durdur (Stop)** düğmesiyle anında kesilebilir.
- **Bağlam ve Geçmiş Temizleme:** Üst çubuktaki **Temizle** düğmesi yerel sohbet geçmişini ve sunucu bağlamını (`POST /v1/makeros/temizle`) sıfırlar.
