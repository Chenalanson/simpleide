#include <stdint.h>
#include "board.h"

/* A deliberately tiny HAL so the demo compiles on a PC.
 * Replace these no-ops with your board's real GPIO/delay code. */

void board_init(void) {
}

void led_set(uint32_t on) {
    (void)on;
}

void delay_ms(uint32_t ms) {
    (void)ms;
}
