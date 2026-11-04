################################################################################
#
# makeros-node
#
################################################################################

MAKEROS_NODE_VERSION = 24.22.2
MAKEROS_NODE_SOURCE = node-v$(MAKEROS_NODE_VERSION)-linux-arm64.tar.xz
MAKEROS_NODE_SITE = https://nodejs.org/dist/v$(MAKEROS_NODE_VERSION)
MAKEROS_NODE_LICENSE = MIT
MAKEROS_NODE_LICENSE_FILES = LICENSE

define MAKEROS_NODE_INSTALL_TARGET_CMDS
	rm -rf $(TARGET_DIR)/opt/node
	mkdir -p $(TARGET_DIR)/opt/node
	cp -a $(@D)/bin $(@D)/include $(@D)/lib $(@D)/LICENSE $(TARGET_DIR)/opt/node/
	ln -sf ../../opt/node/bin/node $(TARGET_DIR)/usr/bin/node
	ln -sf ../../opt/node/bin/npm $(TARGET_DIR)/usr/bin/npm
	ln -sf ../../opt/node/bin/npx $(TARGET_DIR)/usr/bin/npx
	ln -sf ../../opt/node/bin/corepack $(TARGET_DIR)/usr/bin/corepack
endef

$(eval $(generic-package))
