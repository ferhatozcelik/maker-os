################################################################################
#
# chromium-bin
#
################################################################################

# Paket os/chromium/build-bundle.sh ile üretilir (os/out/chromium-bundle-<sürüm>-arm64.tar.xz).
CHROMIUM_BIN_VERSION = $(patsubst chromium-bundle-%-arm64.tar.xz,%,$(notdir $(firstword $(wildcard $(BR2_EXTERNAL_MAKEROS_PATH)/out/chromium-bundle-*-arm64.tar.xz))))
CHROMIUM_BIN_SOURCE = chromium-bundle-$(CHROMIUM_BIN_VERSION)-arm64.tar.xz
CHROMIUM_BIN_SITE = file://$(BR2_EXTERNAL_MAKEROS_PATH)/out
CHROMIUM_BIN_LICENSE = BSD-3-Clause (Chromium), çeşitli (paketlenen kütüphaneler; Debian copyright dosyaları)
CHROMIUM_BIN_INSTALL_STAGING = NO

define CHROMIUM_BIN_EXTRACT_CMDS
	$(TAR) -C $(@D) -xJf $(CHROMIUM_BIN_DL_DIR)/$(CHROMIUM_BIN_SOURCE)
endef

define CHROMIUM_BIN_INSTALL_TARGET_CMDS
	rm -rf $(TARGET_DIR)/opt/chromium
	mkdir -p $(TARGET_DIR)/opt
	cp -a $(@D)/chromium $(TARGET_DIR)/opt/chromium
	# Korumalı alan yardımcısı: kullanıcı ad alanları açıksa gerekmez; setuid bırakılmaz.
	rm -f $(TARGET_DIR)/opt/chromium/chrome-sandbox
	# Grafik yığını imajın kendisinden (Mesa V3D/VC4, wayland 1.24): paketteki eski kopyalar
	# sistem libEGL'i ile çakışıyordu (undefined symbol wl_display_dispatch_queue_timeout).
	cd $(TARGET_DIR)/opt/chromium && rm -rf lib/libwayland-* lib/libgbm.so.1 lib/libEGL_mesa* \
		lib/libGL.so.1 lib/libGLX* lib/libGLdispatch* lib/libgallium-* lib/dri share/glvnd
	$(INSTALL) -D -m 0755 $(CHROMIUM_BIN_PKGDIR)/chromium-launcher $(TARGET_DIR)/opt/chromium/chromium-launcher
	ln -sf /opt/chromium/chromium-launcher $(TARGET_DIR)/usr/bin/chromium
endef

$(eval $(generic-package))
