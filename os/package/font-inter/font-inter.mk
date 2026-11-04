################################################################################
#
# font-inter
#
################################################################################

FONT_INTER_VERSION = 4.1
FONT_INTER_SOURCE = Inter-$(FONT_INTER_VERSION).zip
FONT_INTER_SITE = https://github.com/rsms/inter/releases/download/v$(FONT_INTER_VERSION)
FONT_INTER_LICENSE = OFL-1.1
FONT_INTER_LICENSE_FILES = LICENSE.txt

define FONT_INTER_EXTRACT_CMDS
	$(UNZIP) -q -o $(FONT_INTER_DL_DIR)/$(FONT_INTER_SOURCE) \
		LICENSE.txt InterVariable.ttf InterVariable-Italic.ttf -d $(@D)
endef

define FONT_INTER_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/InterVariable.ttf \
		$(TARGET_DIR)/usr/share/fonts/inter/InterVariable.ttf
	$(INSTALL) -D -m 0644 $(@D)/InterVariable-Italic.ttf \
		$(TARGET_DIR)/usr/share/fonts/inter/InterVariable-Italic.ttf
endef

$(eval $(generic-package))
