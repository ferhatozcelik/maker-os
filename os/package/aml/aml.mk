################################################################################
#
# aml
#
################################################################################

AML_VERSION = 1.0.0
AML_SITE = $(call github,any1,aml,v$(AML_VERSION))
AML_LICENSE = ISC
AML_LICENSE_FILES = COPYING
AML_INSTALL_STAGING = YES
AML_DEPENDENCIES = host-pkgconf
AML_CONF_OPTS = -Dexamples=false

$(eval $(meson-package))
