# Blink demo firmware

A tiny folder-as-project example for Simple embedded.

## Try it

1. Open this folder in Simple embedded.
2. Expand `src` and click `main.c`.
3. Press **Build** or `Ctrl+B`.
4. Check the build output panel at the bottom.

On Windows, the auto-detected build command is `mingw32-make` if MinGW's make is
on `PATH`. On Linux/macOS it will be `make`. The HAL in `src/board.c` is a no-op
mock so the example compiles on a PC.

## Flash

The demo has no Flash command. In **Settings → Flash** you can add something
like `JLink.exe -CommanderScript flash.jlink` for a real board.
