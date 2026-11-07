/* libm pieces compiler_builtins does not export. */
#include <math.h>

long lrint(double x) {
  union { float bits; int value; } result;
  /* vcvtr honours the FPSCR rounding mode, as lrint requires. */
  __asm__("vcvtr.s32.f64 %0, %P1" : "=t"(result.bits) : "w"(x));
  return result.value;
}

double modf(double x, double *integral) {
  double whole = trunc(x);
  *integral = whole;
  if (isinf(x)) return copysign(0.0, x);
  return copysign(x - whole, x);
}

double atanh(double x) {
  return 0.5 * log1p(2.0 * x / (1.0 - x));
}
