# MakerOS AI Asistanı (makeros-ai) — Aşama 1: Türkçe, internetsiz LLM

Raspberry Pi 4 (4 GB) üzerinde tamamen yerel çalışan, Türkçe yanıt veren Linux asistanı. Linux komutları önerir ve
açıklar, hata mesajlarını yorumlar, komutların riskini sınıflandırır. **Hiçbir komutu çalıştırmaz.**

## Bileşenler

| Parça | Ne | Nerede |
|---|---|---|
| Model | Qwen2.5-0.5B-Instruct, GGUF **Q4_K_M** (491 MB, 630 M parametre) | `/var/lib/makeros-ai/models/qwen2.5-0.5b-instruct-q4_k_m.gguf` |
| Çalışma ortamı | llama.cpp **v0.9.0** (`llama-server`, `llama-cli`, `llama-bench`), Cortex-A72 + NEON, OpenMP/curl kapalı | `/usr/bin/llama-*` — `os/package/llama-cpp`, hata ayıklama: `os/debug/build-llama.sh` |
| API + istemci | `makeros-ai` (Rust, makerd paketi; yeni bağımlılık yok) | `makerd/src/ai/`, `makerd/src/bin/makeros-ai.rs` |
| Hizmet | `makeros-ai.service` (`ai.slice`, DynamicUser, kum havuzu, yalnız 127.0.0.1) | `os/board/rpi4/rootfs-overlay/usr/lib/systemd/system/` |
| Masaüstü widget'ı | "AI Asistan" (Ayarlar › Görünüm'den açılıp kapanır) | `shell/apps/ai-widget/`, makerd vekili `/api/v1/ai/*` |

```
 terminal (makeros-ai sohbet/sor) ─┐
 masaüstü widget'ı ── makerd /api/v1/ai (oturum) ─┤
 OpenAI uyumlu istemciler ─────────┴─► makeros-ai 127.0.0.1:8091 ──► llama-server 127.0.0.1:8092 (istek gelince başlar,
                                         · Türkçe sistem istemi + 2 örnek       5 dk boşta kalınca kapanır)
                                         · doğrulanmış komut kartları
                                         · bağlam yönetimi (2048)
                                         · kurallı risk sınıflandırması (SAFE/WARNING/DANGEROUS)
```

## Kurulum

İmajda `llama-cpp` paketi, `makeros-ai` ikilisi ve hizmet birimi hazır gelir. Model (491 MB) imaja konmaz; bir kez kurulur:

```sh
sudo makeros-ai model-kur                      # internetten bir kez indirir, SHA-256 doğrular
sudo makeros-ai model-kur --dosya /media/usb/qwen2.5-0.5b-instruct-q4_k_m.gguf   # internetsiz: USB'den kopyala
sudo systemctl enable --now makeros-ai
```

SHA-256: `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`
(kaynak: `huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF`). Kurulumdan sonra internet gerekmez.

Geliştirme (debug imajı, USB): `os/debug/build-llama.sh && os/debug/push.sh llama` ve `os/debug/push.sh makerd`.

## Kullanım (terminal)

```sh
makeros-ai sohbet                       # etkileşimli; geçmiş tutulur. /yeni, /risk <komut>, /cik
makeros-ai sor "RAM kullanımını kontrol et"
journalctl -p err -b -n 20 | makeros-ai sor "bu hataları açıkla"   # hata çözümleme (boru)
makeros-ai risk "sudo rm -rf /var/log/eski"                        # DANGEROUS (çıkış kodu 20)
makeros-ai durum                        # model yüklü mü, RAM
makeros-ai olcum [--n 8] [--json] [--goster]   # performans raporu
```

## API (OpenAI uyumlu, `http://127.0.0.1:8091`)

| Uç | Açıklama |
|---|---|
| `POST /v1/chat/completions` | Chat Completions; `stream: true` (SSE) desteklenir. Sistem iletisi verilmezse Türkçe Linux asistanı istemi kullanılır; verilirse onunki. Geçmiş `messages` ile gönderilir. |
| `GET /v1/models` | `qwen2.5-0.5b-instruct-q4_k_m` |
| `GET /v1/makeros/durum` | model yüklü mü, PID, RSS, yükleme süresi |
| `POST /v1/makeros/risk` | `{"command": "..."}` ya da `{"text": "<yanıt>"}` → risk değerlendirmesi |
| `POST /v1/makeros/bosalt` | modeli hemen bellekten at |

Yanıta (ve akışta `[DONE]`'dan önceki son parçaya) `x_makeros` eklenir:
`{"commands": [{"command": "df -h", "risk": "SAFE", "reasons": []}], "context": {"dropped_messages": 0, "prompt_tokens_est": 772, ...}}`.
`makeros_bilgi: false` doğrulanmış komut kartlarını kapatır. Varsayılanlar: `temperature 0.2`, `top_p 0.9`,
`repeat_penalty 1.1`, `max_tokens 512` (en çok ctx/2).

```sh
curl -N 127.0.0.1:8091/v1/chat/completions -H 'Content-Type: application/json' \
  -d '{"stream":true,"messages":[{"role":"user","content":"IP adresimi öğren"}]}'
```
```python
from openai import OpenAI
c = OpenAI(base_url="http://127.0.0.1:8091/v1", api_key="yok")
r = c.chat.completions.create(model="qwen2.5-0.5b-instruct-q4_k_m", messages=[{"role": "user", "content": "Disk alanını kontrol et"}])
```

Bağlam yönetimi: istem + `max_tokens` 2048'e sığmalı. Sığmazsa en eski geçmiş iletileri atılır (sistem istemi ve biçim
örnekleri korunur); son ileti tek başına sığmıyorsa ortası kısaltılır (uzun günlüklerin başı ve sonu kalır).

## Güvenlik

- **Komut çalıştırılmaz.** Hizmet kum havuzundadır (DynamicUser, ProtectSystem=strict, ProtectHome, yalnız localhost ağı,
  yetenek yok); kodda komut çalıştıran yol yoktur.
- **Risk düzeyine model karar vermez.** `makerd/src/ai/risk.rs` komutu kabuk sözdizimine göre parçalar
  (`| ; && || & $(…) \`…\``, yönlendirmeler, `sudo/env/nice` önekleri, `find -exec`, `xargs`, `watch`, `sh -c`) ve
  kurallarla sınıflandırır; sonuç en riskli parçadır. Bilinmeyen komut **WARNING** sayılır.
  - SAFE: salt okunur (`df`, `free`, `journalctl`, `systemctl status`, `ip a`, `ping`…)
  - WARNING: durumu değiştirir (`systemctl restart`, paket kurma, dosyaya yazma `>`, `kill`, `sudo`, `reboot`…)
  - DANGEROUS: geri alınamaz (`rm`, `dd`, `mkfs`, `fdisk`, `chmod/chown`, kullanıcı/parola, `curl … | sh`, diske yazma, `find -delete`…)
- Model yanıtındaki komutlar kod bloklarından çıkarılır; örnek çıktı satırları komut sayılmaz.

## Performans (Pi 4, 4 GB, ölçüm: `makeros-ai olcum`, 8 Kasım 2026)

Hizmet `ai.slice` içinde çalışır (CPUQuota %300 → 3 iş parçacığı, CPUWeight 50: masaüstü akıcı kalsın).

| Ölçüm | Değer |
|---|---|
| Model yükleme | 2,1–2,3 sn |
| İlk yanıt, soğuk (model yeni yüklendi, önek önbelleği diskten) | **11,8 sn** (önbelleksiz 55–67 sn) |
| İlk yanıt, sıcak (ort.) | **7,4–8,5 sn** |
| Üretim hızı | **6,7–6,9 token/sn** |
| İstem işleme | 13 token/sn |
| RAM (llama-server RSS) | **564–592 MB**; hizmet boştayken ~5 MB |
| CPU (yanıt sırasında, tüm çekirdekler) | %74–82 |
| Sıcaklık | 54–56 °C (soğutucusuz), voltaj alarmı yok |

Ham llama.cpp (`llama-bench`, kotasız): 4 iş parçacığı pp128 19,5 / tg64 9,3 token/sn; 3 iş parçacığı 14,8 / 8,6.

Sabit önek (sistem istemi + 2 örnek, ~620 token) KV önbelleği `/var/cache/makeros-ai/onek-*.bin` (7,7 MB) olarak
saklanır; model her yüklendiğinde geri yüklenir. İstem değişirse dosya adı değişir, yeniden hesaplanır.

## Bilinen sınırlar

- 0.5B model küçük: biçime örnekler sayesinde uyar, ama zaman zaman yanlış komut önerir, cümleleri tekrarlar ya da
  genel kültür sorularında yanılır. Komut doğruluğu, soruya göre seçilen **doğrulanmış komut kartlarıyla**
  (`makerd/src/ai/prompt.rs`) artırılır; risk etiketi her durumda kurallıdır.
- Q4_K_M, Cortex-A72'de istem işlemede yavaştır (ARM için yeniden paketlenen Q4_0 yaklaşık 2 kat hızlı olur).
- MakerOS'ta `vcgencmd` ve `apt` yoktur; kartlar MakerOS komutlarını önceler, Raspberry Pi OS karşılıklarını alternatif verir.

## Lisanslar

| Bileşen | Lisans | Dosya |
|---|---|---|
| Qwen2.5-0.5B-Instruct (GGUF) | Apache-2.0 (Alibaba Cloud / Qwen) | `/var/lib/makeros-ai/models/LICENSE.qwen` |
| llama.cpp v0.9.0 | MIT | `/usr/share/licenses/llama.cpp/LICENSE` |
| makeros-ai | GPL-3.0-or-later (makerd) | — |

## Sonraki aşamalar (bu teslimatın dışında)

2. STT (whisper.cpp tiny, Türkçe) ve TTS (Piper, Türkçe), ihtiyaçta yüklenen ayrı süreçler.
3. Sesli asistan: STT → LLM → risk katmanı → kullanıcı onayı → denetimli yürütücü → LLM açıklaması → TTS.
