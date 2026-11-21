# Builds the Rust static library and links the C harness against it.
#
# Build outputs go to an external CARGO_TARGET_DIR and a local build/ dir that
# is gitignored, so `git status` stays clean.
#
#   make -C tools/data-ffi          # build
#   make -C tools/data-ffi run      # build and run, showing every check
#   make -C tools/data-ffi clean

WORKSPACE_MANIFEST := ../../rust/Cargo.toml
CRATE_DIR := ../../rust/nookery-data-ffi
CORE_DIR := ../../rust/nookery-core
TARGET_DIR := $(abspath $(CURDIR)/build/target)
BUILD_DIR := $(abspath $(CURDIR)/build)
LIB := $(TARGET_DIR)/release/libnookery_data_ffi.a
HARNESS := $(BUILD_DIR)/harness

CC ?= cc
CFLAGS ?= -std=c11 -O1 -g -Wall -Wextra -Werror -pedantic
# Rust staticlibs need these on glibc targets; harmless where unused.
SYSTEM_LIBS := -lpthread -ldl -lm

.PHONY: all run mutation-test clean

all: $(HARNESS)

$(LIB): $(shell find $(CRATE_DIR)/src $(CORE_DIR)/src -name '*.rs') $(WORKSPACE_MANIFEST) ../../rust/Cargo.lock
	CARGO_TARGET_DIR=$(TARGET_DIR) cargo build --release \
		--manifest-path $(WORKSPACE_MANIFEST) --package nookery-data-ffi

$(HARNESS): harness.c $(LIB)
	mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -I$(CRATE_DIR)/include -o $@ harness.c $(LIB) $(SYSTEM_LIBS)

# The harness reports its own result line; a nonzero exit alone is not evidence.
run: $(HARNESS)
	$(HARNESS)

mutation-test:
	./mutation-test.sh

clean:
	rm -rf $(BUILD_DIR)
