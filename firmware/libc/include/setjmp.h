#ifndef FUJI_SETJMP_H
#define FUJI_SETJMP_H

/* r4-r11, sp, lr, then d8-d15. */
typedef unsigned long long jmp_buf[13];

int setjmp(jmp_buf env) __attribute__((returns_twice));
void longjmp(jmp_buf env, int value) __attribute__((noreturn));

#endif
