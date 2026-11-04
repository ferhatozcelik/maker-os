# MakerOS kabuğu — yalnız Docker ile geliştirme/test (ana makinede node/Chrome gerekmez).
#   make dev          sahte API'li dev-server → http://localhost/ (maker / maker)
#   make dev-fresh    aynı, ilk açılış (kurulum sihirbazı)
#   make tokens       tokens.json → css/tokens.css
#   make smoke        dev-server'a karşı duman testi (ekran görüntüleri: OUT)
#   make smoke-fresh  --fresh dev-server'a karşı sihirbaz + tam test
#   make smoke-makerd gerçek makerd --mock (MAKERD_MOCK_FRESH=1) kapsayıcısına karşı --fresh test
#   make vnc-e2e      uçtan uca Uzak Ekran (noVNC → makerd → wayvnc → labwc)
#   make down         kapsayıcıları ve ağı kaldır
SHELL_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
REPO      := $(abspath $(SHELL_DIR)/..)
OUT       ?= $(SHELL_DIR)/.smoke
NODE_IMG  ?= node:24-alpine
SMOKE_IMG ?= makeros/shell-smoke:dev
MAKERD_IMG ?= makeros/makerd:dev
NET       := makeros-shell
UID_GID   := $(shell id -u):$(shell id -g)
COMPOSE   ?= $(shell docker compose version >/dev/null 2>&1 && echo "docker compose" || echo docker-compose)

.PHONY: smoke-run net dev dev-fresh tokens smoke-image smoke smoke-fresh makerd-fresh smoke-makerd down
net:
	@docker network inspect $(NET) >/dev/null 2>&1 || docker network create $(NET) >/dev/null

dev: net
	-@docker rm -f mk-shell-dev >/dev/null 2>&1
	docker run -d --name mk-shell-dev --network $(NET) -p 80:8090 -v "$(SHELL_DIR)":/shell:ro $(NODE_IMG) node /shell/tools/dev-server.mjs $(ARGS)
	@echo "→ http://localhost/"

dev-fresh:
	$(MAKE) dev ARGS=--fresh

tokens:
	docker run --rm --user $(UID_GID) -v "$(SHELL_DIR)":/shell -w /shell $(NODE_IMG) node tools/gen-tokens.mjs

smoke-image:
	docker build -q -f "$(SHELL_DIR)/tools/docker/Dockerfile.smoke" -t $(SMOKE_IMG) "$(SHELL_DIR)/tools/docker" >/dev/null

# URL: kapsayıcı ağındaki hedef; FLAGS: ör. --fresh
smoke-run: smoke-image net
	@mkdir -p "$(OUT)"
	docker run --rm --network $(NET) --shm-size=1g -e HOME=/tmp -v "$(SHELL_DIR)":/shell:ro -v "$(OUT)":/out \
	  $(SMOKE_IMG) --url $(URL) --out /out $(FLAGS)

smoke: dev
	@sleep 1
	$(MAKE) smoke-run URL=http://mk-shell-dev:8090

smoke-fresh: dev-fresh
	@sleep 1
	$(MAKE) smoke-run URL=http://mk-shell-dev:8090 FLAGS=--fresh

makerd-fresh: net
	cd "$(REPO)" && $(COMPOSE) build makerd
	-@docker rm -f mk-makerd >/dev/null 2>&1
	docker run -d --name mk-makerd --network $(NET) -p 8080:8080 -e MAKERD_MOCK_FRESH=1 --read-only --tmpfs /tmp \
	  --tmpfs /home/maker:uid=1000,gid=1000,mode=0755 -v "$(SHELL_DIR)":/web:ro $(MAKERD_IMG) --mock --listen :8080 --web /web --log-level info

smoke-makerd: makerd-fresh
	@sleep 2
	$(MAKE) smoke-run URL=http://mk-makerd:8080 FLAGS="--fresh --makerd"

down:
	-docker rm -f mk-shell-dev mk-makerd
	-docker network rm $(NET)

# Uçtan uca Uzak Ekran: depo kökündeki compose (makerd --mock + labwc/wayvnc sanal masaüstü)
# → başsız Chromium noVNC ile bağlanır, VNC'den terminale komut yazar, sonucu masaüstü kabında doğrular.
.PHONY: vnc-e2e
vnc-e2e: smoke-image
	cd "$(REPO)" && $(COMPOSE) up -d --build --force-recreate
	@sleep 5
	docker run --rm --network maker-os_default -v "$(SHELL_DIR)":/shell:ro -v "$(OUT)":/out --entrypoint node $(SMOKE_IMG) /shell/tools/vnc-e2e.mjs --url http://makerd:8080 --out /out
	cd "$(REPO)" && $(COMPOSE) exec -T desktop test -f /tmp/vnc-e2e-ok && echo "✔ komut uzak masaüstünde çalıştı"
