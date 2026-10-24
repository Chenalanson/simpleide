#ifndef BOARD_H
#define BOARD_H

#include <stdint.h>

void board_init(void);
void led_set(uint32_t on);
void delay_ms(uint32_t ms);

#endif
