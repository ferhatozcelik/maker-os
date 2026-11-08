################################################################################
#
# makeros-shell
#
################################################################################

MAKEROS_SHELL_VERSION = $(MAKEROS_VERSION)
MAKEROS_SHELL_SITE = $(BR2_EXTERNAL_MAKEROS_PATH)/../shell
MAKEROS_SHELL_SITE_METHOD = local
MAKEROS_SHELL_LICENSE = MIT
MAKEROS_SHELL_DEST = $(TARGET_DIR)/usr/share/makeros/shell
# Gelistirme dosyalari imaja girmez.
MAKEROS_SHELL_EXCLUDES = tools node_modules tests test '*.md' '.*'

define MAKEROS_SHELL_INSTALL_TARGET_CMDS
	rm -rf $(MAKEROS_SHELL_DEST)
	mkdir -p $(MAKEROS_SHELL_DEST)
	rsync -a $(foreach e,$(MAKEROS_SHELL_EXCLUDES),--exclude=$(e)) \
		$(@D)/ $(MAKEROS_SHELL_DEST)/
	find $(MAKEROS_SHELL_DEST) -type f \
		\( -name '*.js' -o -name '*.mjs' -o -name '*.css' -o -name '*.html' \
		   -o -name '*.svg' -o -name '*.json' \) -size +1k \
		-exec gzip -9 -n -k -f {} \;
	find $(MAKEROS_SHELL_DEST) -type d -exec chmod 0755 {} +
	find $(MAKEROS_SHELL_DEST) -type f -exec chmod 0644 {} +
endef

$(eval $(generic-package))
