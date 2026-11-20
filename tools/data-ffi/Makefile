# Builds the Rust static library and links the C harness against it.
#
# Build outputs go to an external CARGO_TARGET_DIR and a local build/ dir that
# is gitignored, so `git status` stays clean.
#
#   make -C tools/data-ffi          # build
#   make -C tools/data-ffi run      # build and run, showing every check
#   make -C tools/data-ffi clean

CRATE_DIR := ../../rust/nookery-data-ffi
TARGET_DIR := $(abspath $(CURDIR)/build/target)
BUILD_DIR := $(abspath $(CURDIR)/build)
LIB := $(TARGET_DIR)/release/libnookery_data_ffi.a
HARNESS := $(BUILD_DIR)/harness

CC ?= cc
CFLAGS ?= -std=c11 -O1 -g -Wall -Wextra -Werror -pedantic
# Rust staticlibs need these on glibc targets; harmless where unused.
SYSTEM_LIBS := -lpthread -ldl -lm

.PHONY: all run clean

all: $(HARNESS)

$(LIB): $(shell find $(CRATE_DIR)/src -name '*.rs') $(CRATE_DIR)/Cargo.toml
	CARGO_TARGET_DIR=$(TARGET_DIR) cargo build --release \
		--manifest-path $(CRATE_DIR)/Cargo.toml

$(HARNESS): harness.c $(LIB)
	mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -I$(CRATE_DIR)/include -o $@ harness.c $(LIB) $(SYSTEM_LIBS)

# The harness reports its own result line; a nonzero exit alone is not evidence.
run: $(HARNESS)
	$(HARNESS)

clean:
	rm -rf $(BUILD_DIR)
