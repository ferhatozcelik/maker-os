################################################################################
#
# makerd
#
################################################################################

MAKERD_VERSION = $(MAKEROS_VERSION)
MAKERD_SITE = $(BR2_EXTERNAL_MAKEROS_PATH)/../makerd
MAKERD_SITE_METHOD = local
MAKERD_LICENSE = GPL-3.0-or-later
MAKERD_GOMOD = $(or $(shell sed -n 's/^module[[:space:]]*//p' $(MAKERD_SITE)/go.mod 2>/dev/null),github.com/ferhatozcelik/maker-os/makerd)
MAKERD_BUILD_TARGETS = cmd/makerd cmd/makerd-priv cmd/makerd-user
MAKERD_GO_ENV = CGO_ENABLED=0
MAKERD_LDFLAGS = -s -w -X main.Version=$(MAKEROS_VERSION)

# 'local' kaynakta Buildroot'un go vendoring adimi (download post-process)
# calismaz. vendor/ yoksa modulleri burada (agla) indirip vendor'layalim.
# Ayrica go.mod'daki 'go 1.26.N' satiri Buildroot'un host-go surumunden
# (GOTOOLCHAIN=local) yeniyse derleme reddedilir; host-go surumune indir.
define MAKERD_VENDOR_MODULES
	cd $(@D) && \
	if [ -n "$$(sed -n 's/^go[[:space:]]\+//p' go.mod)" ]; then \
		$(HOST_GO_COMMON_ENV) GOFLAGS=-mod=mod $(GO_BIN) mod edit -go=$(GO_VERSION) -toolchain=none; \
	fi && \
	if [ ! -d vendor ]; then \
		$(HOST_GO_COMMON_ENV) GOFLAGS=-mod=mod GOPROXY=https://proxy.golang.org,direct \
			$(GO_BIN) mod vendor -modcacherw; \
	fi
endef
MAKERD_PRE_BUILD_HOOKS += MAKERD_VENDOR_MODULES

# Birimler makerd agacindan (makerd/dist) gelir; makerd 'maker' kullanicisiyla
# calisir, /run/makeros (belirtec + priv.sock) tmpfiles ile olusur.
define MAKERD_INSTALL_INIT_SYSTEMD
	$(INSTALL) -D -m 0644 $(@D)/dist/makerd.service \
		$(TARGET_DIR)/usr/lib/systemd/system/makerd.service
	$(INSTALL) -D -m 0644 $(@D)/dist/makerd-priv.socket \
		$(TARGET_DIR)/usr/lib/systemd/system/makerd-priv.socket
	$(INSTALL) -D -m 0644 $(@D)/dist/makerd-priv.service \
		$(TARGET_DIR)/usr/lib/systemd/system/makerd-priv.service
	$(INSTALL) -D -m 0644 $(@D)/dist/makerd-user@.service \
		$(TARGET_DIR)/usr/lib/systemd/system/makerd-user@.service
	$(INSTALL) -D -m 0644 $(@D)/dist/makeros.tmpfiles.conf \
		$(TARGET_DIR)/usr/lib/tmpfiles.d/makerd.conf
	$(INSTALL) -D -m 0644 $(MAKERD_PKGDIR)/50-makerd.rules \
		$(TARGET_DIR)/usr/share/polkit-1/rules.d/50-makerd.rules
endef

$(eval $(golang-package))
