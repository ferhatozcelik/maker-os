################################################################################
#
# makeros-python-tkinter
#
################################################################################

# Kaynak: python3 paketinin indirdigi CPython arsivi (ayni surum, ayni dl/ dizini).
MAKEROS_PYTHON_TKINTER_VERSION = $(PYTHON3_VERSION)
MAKEROS_PYTHON_TKINTER_SOURCE = $(PYTHON3_SOURCE)
MAKEROS_PYTHON_TKINTER_SITE = $(PYTHON3_SITE)
MAKEROS_PYTHON_TKINTER_DL_SUBDIR = python3
MAKEROS_PYTHON_TKINTER_LICENSE = Python-2.0
MAKEROS_PYTHON_TKINTER_LICENSE_FILES = LICENSE
MAKEROS_PYTHON_TKINTER_DEPENDENCIES = python3 tcl tk

MAKEROS_PYTHON_TKINTER_PYDIR = usr/lib/python$(PYTHON3_VERSION_MAJOR)

define MAKEROS_PYTHON_TKINTER_BUILD_CMDS
	$(TARGET_CC) $(TARGET_CFLAGS) -fPIC -shared -DWITH_APPINIT=1 \
		-I$(@D)/Modules -I$(STAGING_DIR)/usr/include/python$(PYTHON3_VERSION_MAJOR) \
		-I$(STAGING_DIR)/usr/include/python$(PYTHON3_VERSION_MAJOR)/internal \
		$(@D)/Modules/_tkinter.c $(@D)/Modules/tkappinit.c \
		$(TARGET_LDFLAGS) -ltk$(TK_VERSION_MAJOR) -ltcl$(TCL_VERSION_MAJOR) -lX11 \
		-o $(@D)/_tkinter.so
endef

define MAKEROS_PYTHON_TKINTER_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/_tkinter.so \
		$(TARGET_DIR)/$(MAKEROS_PYTHON_TKINTER_PYDIR)/lib-dynload/_tkinter.so
	rm -rf $(TARGET_DIR)/$(MAKEROS_PYTHON_TKINTER_PYDIR)/tkinter
	cp -a $(@D)/Lib/tkinter $(TARGET_DIR)/$(MAKEROS_PYTHON_TKINTER_PYDIR)/tkinter
endef

$(eval $(generic-package))
