# MakerOS BR2_EXTERNAL: tum ozel paketleri dahil et.
include $(sort $(wildcard $(BR2_EXTERNAL_MAKEROS_PATH)/package/*/*.mk))

# Surum bilgisi (os/VERSION) post-build ve paketler tarafindan kullanilir.
MAKEROS_VERSION := $(shell cat $(BR2_EXTERNAL_MAKEROS_PATH)/VERSION 2>/dev/null || echo 0.0.0)
