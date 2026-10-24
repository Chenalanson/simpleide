#include <stdint.h>
#include "board.h"

/* Minimal blink example for MiniEIDE.
 * Open this folder, then press Build (Ctrl+B). */
int main(void) {
    board_init();

    for (;;) {
        led_set(1);
        delay_ms(250);
        led_set(0);
        delay_ms(250);
    }
}
