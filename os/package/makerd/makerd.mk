################################################################################
#
# makerd
#
################################################################################

MAKERD_VERSION = $(MAKEROS_VERSION)
MAKERD_SITE = $(BR2_EXTERNAL_MAKEROS_PATH)/../makerd
MAKERD_SITE_METHOD = local
MAKERD_LICENSE = MIT
MAKERD_CARGO_ENV = MAKEROS_VERSION=$(MAKEROS_VERSION)
MAKERD_CARGO_BUILD_OPTS = --bins

# 'local' kaynakta Buildroot'un cargo vendoring adimi (download post-process)
# calismaz; derleme --offline calistigi icin bagimliliklari burada VENDOR/
# altina indirip .cargo/config.toml olusturalim.
define MAKERD_VENDOR_CRATES
	cd $(@D) && \
	if [ ! -d VENDOR ]; then \
		mkdir -p .cargo $(BR_CARGO_HOME) && \
		$(TARGET_MAKE_ENV) $(PKG_CARGO_ENV) \
			cargo vendor --manifest-path Cargo.toml --locked VENDOR > .cargo/config.toml; \
	fi
endef
MAKERD_PRE_BUILD_HOOKS += MAKERD_VENDOR_CRATES

define MAKERD_INSTALL_TARGET_CMDS
	for b in makerd makerd-priv makerd-user makeros-ai; do \
		$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/release/$$b \
			$(TARGET_DIR)/usr/bin/$$b || exit 1; \
	done
endef

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

$(eval $(cargo-package))
