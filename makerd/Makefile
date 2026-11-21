# makerd (Rust) — yalnız Docker. Ana makineye Rust kurulmaz.
#   make preview   derle + makerd --mock ve web kabuğu → http://localhost/ (maker / maker)
#   make fresh     aynısı, ilk açılış sihirbazıyla (kullanıcı yok)
#   make build     yalnız derle (dev profili; önbellek: mk-cargo, mk-target birimleri)
#   make test      cargo test
#   make stop      önizlemeyi durdur
#   make ai        yerel AI asistanı (makeros-ai + llama-server; model ilk seferde indirilir, ~400 MB)
#                  preview bunu kendiliğinden çalıştırır; kapatmak için: make preview AI=0
# JOBS: aynı anda derlenen crate sayısı (makineyi boğmamak için düşük tutulur).
RUST_IMG ?= rust:1.88-slim-bookworm
# Çalışma imajı: aynı Debian (glibc) + uv + Python; eklentiler ve Python yöneticisi gerçek uv ile çalışır.
RUN_IMG  ?= ghcr.io/astral-sh/uv:python3.13-bookworm-slim
# AI imajı: llama-server (/app/llama-server) hazır gelir; makeros-ai mk-target biriminden çalışır.
AI_IMG   ?= ghcr.io/ggml-org/llama.cpp:server
AI       ?= 1
JOBS     ?= 3
ROOT     := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))..)
CARGO    := docker run --rm -v "$(ROOT)/makerd":/src -v mk-cargo:/usr/local/cargo/registry -v mk-target:/target \
            -e CARGO_TARGET_DIR=/target -w /src $(RUST_IMG) cargo

.PHONY: build test preview fresh stop logs ai ai-build
build:
	$(CARGO) build --locked --bin makerd $(if $(filter 1,$(AI)),--bin makeros-ai) -j $(JOBS)

test:
	$(CARGO) test --locked -j $(JOBS)

preview: build
	-@docker rm -f mk-ai mk-makerd mk-shell-dev >/dev/null 2>&1
	docker run -d --name mk-makerd -p 80:8080 --user 1000:1000 -e HOME=/tmp/home -e MAKERD_MOCK_FRESH=$(FRESH) \
	  -v mk-target:/target:ro -v "$(ROOT)/shell":/web:ro --tmpfs /tmp:exec,mode=1777 \
	  $(RUN_IMG) sh -c 'mkdir -p $$HOME && exec /target/debug/makerd --mock --listen :8080 --web /web'
	@if [ "$(AI)" = "1" ]; then $(MAKE) --no-print-directory ai-run; fi
	@echo "→ http://localhost/  (maker / maker)"

fresh:
	$(MAKE) preview FRESH=1

ai-build:
	$(CARGO) build --locked --bin makeros-ai -j $(JOBS)

ai: ai-build ai-run

# makeros-ai, mk-makerd ile aynı ağ ad alanında çalışır: makerd /api/v1/ai/ isteklerini 127.0.0.1:8091'e iletir.
ai-run:
	-@docker rm -f mk-ai >/dev/null 2>&1
	docker run --rm -v mk-target:/target:ro -v mk-ai-models:/var/lib/makeros-ai \
	  --entrypoint /target/debug/makeros-ai $(AI_IMG) model-kur
	docker run -d --name mk-ai --network container:mk-makerd -w /app \
	  -v mk-target:/target:ro -v mk-ai-models:/var/lib/makeros-ai:ro \
	  --entrypoint /target/debug/makeros-ai $(AI_IMG) serve --listen 127.0.0.1:8091 --llama /app/llama-server --cache-dir /tmp --idle 5m

stop:
	-docker rm -f mk-ai mk-makerd

logs:
	docker logs -f mk-makerd

ai-logs:
	docker logs -f mk-ai
