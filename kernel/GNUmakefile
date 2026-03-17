MAKEFLAGS += -rR
.SUFFIXES:

override OUTPUT := kernel
override USER_VARIABLE = $(if $(filter $(origin $(1)),default undefined),$(eval override $(1) := $(2)))
$(call USER_VARIABLE,KARCH,x86_64)

override RUST_TARGET := $(KARCH)-unknown-none
override RUST_PROFILE := dev
override RUST_PROFILE_SUBDIR := $(if $(filter dev,$(RUST_PROFILE)),debug,$(RUST_PROFILE))

.PHONY: all
all:
	RUSTFLAGS="-C relocation-model=static" cargo build --target $(RUST_TARGET) --profile $(RUST_PROFILE)
	cp target/$(RUST_TARGET)/$(RUST_PROFILE_SUBDIR)/ts-os-kernel kernel

.PHONY: clean
clean:
	cargo clean
	rm -f kernel

.PHONY: distclean
distclean: clean
