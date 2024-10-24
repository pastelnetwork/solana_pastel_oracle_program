# libgmp.mk - Build rules for GMP

package=libgmp
$(package)_version=6.2.1
$(package)_download_path=https://gmplib.org/download/gmp
$(package)_file_name=gmp-$(libgmp_version).tar.lz
$(package)_sha256_hash=2c7f4f0d370801b2849c48c9ef3f59553b5f1d3791d070cffb04599f9fc67b41
$(package)_dependencies=

define $(package)_build_cmds
    echo "BUILD_DIR is: $(BUILD_DIR)"
	cd \$(BUILD_DIR) && patch -p1 < \$(abs_top_srcdir)/depends/patches/libgmp/disable-asm-for-arm64.patch
	cd \$(BUILD_DIR) && ./configure --prefix=\$(PREFIX) \$(HOST) --enable-shared --enable-static
	cd \$(BUILD_DIR) && make -j\$(JOBS)
	cd \$(BUILD_DIR) && make install
endef
