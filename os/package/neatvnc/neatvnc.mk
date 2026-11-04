################################################################################
#
# neatvnc
#
################################################################################

NEATVNC_VERSION = 1.0.4
NEATVNC_SITE = $(call github,any1,neatvnc,v$(NEATVNC_VERSION))
NEATVNC_LICENSE = ISC
NEATVNC_LICENSE_FILES = COPYING
NEATVNC_INSTALL_STAGING = YES
NEATVNC_DEPENDENCIES = host-pkgconf aml pixman zlib libdrm
NEATVNC_CONF_OPTS = -Dexamples=false -Dtests=false -Dbenchmarks=false

# Kimlik dogrulama makerd'de (oturum cerezi) yapilir; wayvnc yalnizca
# 127.0.0.1'i dinler. TLS/nettle bu yuzden gerekmez.
NEATVNC_CONF_OPTS += -Dtls=disabled -Dnettle=disabled

ifeq ($(BR2_PACKAGE_JPEG_TURBO),y)
NEATVNC_DEPENDENCIES += jpeg
NEATVNC_CONF_OPTS += -Djpeg=enabled
else
NEATVNC_CONF_OPTS += -Djpeg=disabled
endif

ifeq ($(BR2_PACKAGE_HAS_LIBGBM),y)
NEATVNC_DEPENDENCIES += libgbm
NEATVNC_CONF_OPTS += -Dgbm=enabled
else
NEATVNC_CONF_OPTS += -Dgbm=disabled
endif

ifeq ($(BR2_PACKAGE_FFMPEG)$(BR2_PACKAGE_HAS_LIBGBM),yy)
NEATVNC_DEPENDENCIES += ffmpeg
NEATVNC_CONF_OPTS += -Dh264=enabled
else
NEATVNC_CONF_OPTS += -Dh264=disabled
endif

$(eval $(meson-package))
