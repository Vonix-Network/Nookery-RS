CC ?= cc
CFLAGS ?= -O1 -g -std=c11 -Wall -Wextra -Werror -pedantic
BUILD_DIR ?= build

.PHONY: all run clean

all: $(BUILD_DIR)/yaz0_oracle

$(BUILD_DIR)/yaz0_oracle: yaz0_oracle.c
	mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -o $@ $<

# Requires a user-supplied .szs file; never commit game data.
run: all
	@if [ -z "$(SZS)" ]; then echo "usage: make run SZS=<file.szs>"; exit 2; fi
	$(BUILD_DIR)/yaz0_oracle "$(SZS)"

clean:
	rm -rf $(BUILD_DIR)
