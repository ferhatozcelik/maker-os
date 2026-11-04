# makerd (Rust) — yalnız Docker. Ana makineye Rust kurulmaz.
#   make preview   derle + makerd --mock ve web kabuğu → http://localhost/ (maker / maker)
#   make fresh     aynısı, ilk açılış sihirbazıyla (kullanıcı yok)
#   make build     yalnız derle (dev profili; önbellek: mk-cargo, mk-target birimleri)
#   make test      cargo test
#   make stop      önizlemeyi durdur
# JOBS: aynı anda derlenen crate sayısı (makineyi boğmamak için düşük tutulur).
RUST_IMG ?= rust:1.88-slim-bookworm
# Çalışma imajı: aynı Debian (glibc) + uv + Python; eklentiler ve Python yöneticisi gerçek uv ile çalışır.
RUN_IMG  ?= ghcr.io/astral-sh/uv:python3.13-bookworm-slim
JOBS     ?= 3
ROOT     := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))..)
CARGO    := docker run --rm -v "$(ROOT)/makerd-rs":/src -v mk-cargo:/usr/local/cargo/registry -v mk-target:/target \
            -e CARGO_TARGET_DIR=/target -w /src $(RUST_IMG) cargo

.PHONY: build test preview fresh stop logs
build:
	$(CARGO) build --locked --bin makerd -j $(JOBS)

test:
	$(CARGO) test --locked -j $(JOBS)

preview: build
	-@docker rm -f mk-makerd mk-shell-dev >/dev/null 2>&1
	docker run -d --name mk-makerd -p 80:8080 --user 1000:1000 -e HOME=/tmp/home -e MAKERD_MOCK_FRESH=$(FRESH) \
	  -v mk-target:/target:ro -v "$(ROOT)/shell":/web:ro --tmpfs /tmp:exec,mode=1777 \
	  $(RUN_IMG) sh -c 'mkdir -p $$HOME && exec /target/debug/makerd --mock --listen :8080 --web /web'
	@echo "→ http://localhost/  (maker / maker)"

fresh:
	$(MAKE) preview FRESH=1

stop:
	-docker rm -f mk-makerd

logs:
	docker logs -f mk-makerd
