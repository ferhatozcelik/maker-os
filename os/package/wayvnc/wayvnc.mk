################################################################################
#
# wayvnc
#
################################################################################

WAYVNC_VERSION = 0.10.2
WAYVNC_SITE = $(call github,any1,wayvnc,v$(WAYVNC_VERSION))
WAYVNC_LICENSE = ISC
WAYVNC_LICENSE_FILES = COPYING
WAYVNC_DEPENDENCIES = host-pkgconf host-wayland aml neatvnc jansson \
	libxkbcommon pixman wayland libdrm
WAYVNC_CONF_OPTS = -Dman-pages=disabled -Dtests=false -Dpam=disabled

ifeq ($(BR2_PACKAGE_HAS_LIBGBM),y)
WAYVNC_DEPENDENCIES += libgbm
WAYVNC_CONF_OPTS += -Dscreencopy-dmabuf=enabled
else
WAYVNC_CONF_OPTS += -Dscreencopy-dmabuf=disabled
endif

$(eval $(meson-package))
