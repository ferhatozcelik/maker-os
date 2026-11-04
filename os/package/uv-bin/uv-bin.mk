################################################################################
#
# uv-bin
#
################################################################################

UV_BIN_VERSION = 0.13.6
UV_BIN_SOURCE = uv-aarch64-unknown-linux-gnu.tar.gz
UV_BIN_SITE = https://github.com/astral-sh/uv/releases/download/$(UV_BIN_VERSION)
UV_BIN_LICENSE = Apache-2.0 or MIT
# Tarball tek bir uv-aarch64-unknown-linux-gnu/ dizini icerir.
UV_BIN_STRIP_COMPONENTS = 1

define UV_BIN_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/uv $(TARGET_DIR)/usr/bin/uv
	$(INSTALL) -D -m 0755 $(@D)/uvx $(TARGET_DIR)/usr/bin/uvx
endef

$(eval $(generic-package))
