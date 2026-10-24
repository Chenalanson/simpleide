# Blink demo (folder-as-project)
#
# Build with MinGW's mingw32-make on Windows, or GNU make elsewhere.
# This example only compiles; the HAL is a no-op mock.

CC      ?= gcc
CFLAGS  ?= -std=c11 -Wall -Wextra -O2 -Iinclude
SOURCES  = src/main.c src/board.c
TARGET   = build/firmware

ifeq ($(OS),Windows_NT)
TARGET   := $(TARGET).exe
MKDIR    := if not exist build mkdir build
CLEAN    := if exist build rmdir /s /q build
else
MKDIR    := mkdir -p build
CLEAN    := rm -rf build
endif

.PHONY: all clean

all: $(TARGET)

$(TARGET): $(SOURCES) include/board.h
	@$(MKDIR)
	$(CC) $(CFLAGS) $(SOURCES) -o $@

clean:
	@$(CLEAN)
