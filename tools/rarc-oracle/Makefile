TARGET := build/rarc_oracle

.PHONY: all clean

all: $(TARGET)

$(TARGET): main.rs
	mkdir -p build
	rustc --edition=2024 -O main.rs -o $(TARGET)

clean:
	rm -rf build
