#ifndef FUJI_FENV_H
#define FUJI_FENV_H

/* VFP FPSCR.RMode values (bits 22-23). */
#define FE_TONEAREST 0x000000
#define FE_UPWARD 0x400000
#define FE_DOWNWARD 0x800000
#define FE_TOWARDZERO 0xc00000

int fegetround(void);
int fesetround(int mode);

#endif
