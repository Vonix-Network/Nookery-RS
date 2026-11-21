CRATE_DIR := ../../rust/nookery-config
INCLUDE_DIR := $(CRATE_DIR)/include
ORACLE_INCLUDE_DIR := ../../pc/include
TARGET_DIR ?= /tmp/nookery-settings-abi-target
BUILD_DIR ?= /tmp/nookery-settings-abi-build
LIB := $(TARGET_DIR)/release/libnookery_config.a
HARNESS := $(BUILD_DIR)/harness

CC ?= cc
CFLAGS ?= -std=c11 -O1 -g -Wall -Wextra -Werror -pedantic
SYSTEM_LIBS := -lpthread -ldl -lm

.PHONY: all run clean

all: $(HARNESS)

$(LIB): $(shell find $(CRATE_DIR)/src -name '*.rs') $(CRATE_DIR)/Cargo.toml
	CARGO_TARGET_DIR=$(TARGET_DIR) cargo build --release --manifest-path $(CRATE_DIR)/Cargo.toml

$(HARNESS): harness.c $(LIB) $(INCLUDE_DIR)/nookery_config.h $(ORACLE_INCLUDE_DIR)/pc_settings.h
	mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -I$(ORACLE_INCLUDE_DIR) -I$(INCLUDE_DIR) -o $@ harness.c $(LIB) $(SYSTEM_LIBS)

run: $(HARNESS)
	$(HARNESS)

clean:
	rm -rf $(BUILD_DIR)
