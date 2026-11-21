################################################################################
#
# makeros-llama-cpp — yerel LLM çıkarımı (makeros-ai için llama-server)
# Buildroot'un kendi llama-cpp paketiyle çakışmasın diye ayrı ad; Pi 4'e özel derleme seçenekleri
#
################################################################################

MAKEROS_LLAMA_CPP_VERSION = 0.9.0
# GitHub etiketi v0.9.0 (Buildroot github yardımcısı: .../archive/v0.9.0/makeros-llama-cpp-0.9.0.tar.gz)
MAKEROS_LLAMA_CPP_SITE = $(call github,ggml-org,llama.cpp,v$(MAKEROS_LLAMA_CPP_VERSION))
MAKEROS_LLAMA_CPP_LICENSE = MIT
MAKEROS_LLAMA_CPP_LICENSE_FILES = LICENSE
MAKEROS_LLAMA_CPP_INSTALL_STAGING = NO

# Pi 4 (Cortex-A72): NEON + CRC; dotprod/i8mm yok. OpenMP kapalı (imajda libgomp yok, ggml kendi havuzunu
# kullanır), curl kapalı (model yerel dosyadan), statik kitaplıklar (yalnız üç ikili kurulur).
MAKEROS_LLAMA_CPP_CONF_OPTS = \
	-DBUILD_SHARED_LIBS=OFF \
	-DGGML_NATIVE=OFF \
	-DGGML_CPU_ARM_ARCH=armv8-a+crc \
	-DGGML_OPENMP=OFF \
	-DLLAMA_CURL=OFF \
	-DLLAMA_BUILD_TESTS=OFF \
	-DLLAMA_BUILD_EXAMPLES=OFF \
	-DLLAMA_BUILD_TOOLS=ON \
	-DLLAMA_BUILD_SERVER=ON \
	-DCMAKE_C_FLAGS="$(TARGET_CFLAGS) -mcpu=cortex-a72 -O3" \
	-DCMAKE_CXX_FLAGS="$(TARGET_CXXFLAGS) -mcpu=cortex-a72 -O3"

define MAKEROS_LLAMA_CPP_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(BR2_CMAKE) --build $(MAKEROS_LLAMA_CPP_BUILDDIR) -j$(PARALLEL_JOBS) \
		--target llama-server llama-cli
endef

define MAKEROS_LLAMA_CPP_INSTALL_TARGET_CMDS
	for b in llama-server llama-cli; do \
		$(INSTALL) -D -m 0755 $(MAKEROS_LLAMA_CPP_BUILDDIR)/bin/$$b $(TARGET_DIR)/usr/bin/$$b; \
	done
	$(INSTALL) -D -m 0644 $(@D)/LICENSE $(TARGET_DIR)/usr/share/licenses/llama.cpp/LICENSE
endef

$(eval $(cmake-package))
