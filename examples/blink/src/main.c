#include <stdint.h>
#include "board.h"

/* Minimal blink example for Simple embedded.
 * Open this folder, then press Build (Ctrl+B). */
int main(void) {
    board_init();
    uint8_t i = 0;

    for (;;) {
        led_set(1);
        delay_ms(250);
        led_set(0);
        delay_ms(250);
    }
}

//TODO: 你好呀
