################################################################################
#
# labwc
#
################################################################################

# 0.9.x serisi wlroots 0.19 ile (Buildroot 2026.02: wlroots 0.19.2) derlenir;
# 0.20.x wlroots 0.20 ister.
LABWC_VERSION = 0.9.8
LABWC_SITE = $(call github,labwc,labwc,$(LABWC_VERSION))
LABWC_LICENSE = GPL-2.0-only
LABWC_LICENSE_FILES = LICENSE
LABWC_DEPENDENCIES = host-pkgconf host-wayland wlroots wayland-protocols \
	libxml2 libglib2 cairo pango libpng libinput libxkbcommon pixman libdrm

# svg dugmeler librsvg (Rust) ister, simgeler libsfdo ister: ikisi de kapali,
# imaj kucuk ve derleme kisa kalir. Xwayland M3'te (OpenCV imshow) acilacak.
LABWC_CONF_OPTS = -Dman-pages=disabled -Dsvg=disabled -Dicon=disabled \
	-Dxwayland=disabled -Dnls=disabled

$(eval $(meson-package))
