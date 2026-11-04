################################################################################
#
# makeros-firstboot
#
################################################################################

MAKEROS_FIRSTBOOT_VERSION = $(MAKEROS_VERSION)
MAKEROS_FIRSTBOOT_SITE = $(MAKEROS_FIRSTBOOT_PKGDIR)/files
MAKEROS_FIRSTBOOT_SITE_METHOD = local
MAKEROS_FIRSTBOOT_LICENSE = GPL-3.0-or-later

define MAKEROS_FIRSTBOOT_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/makeros-firstboot \
		$(TARGET_DIR)/usr/lib/makeros/firstboot
endef

define MAKEROS_FIRSTBOOT_INSTALL_INIT_SYSTEMD
	$(INSTALL) -D -m 0644 $(@D)/makeros-firstboot.service \
		$(TARGET_DIR)/usr/lib/systemd/system/makeros-firstboot.service
endef

$(eval $(generic-package))
