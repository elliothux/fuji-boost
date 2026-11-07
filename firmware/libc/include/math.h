#ifndef FUJI_MATH_H
#define FUJI_MATH_H

#define NAN __builtin_nan("")
#define INFINITY __builtin_inf()
#define HUGE_VAL __builtin_huge_val()
#define HUGE_VALF __builtin_huge_valf()

#define FP_NAN 0
#define FP_INFINITE 1
#define FP_ZERO 2
#define FP_SUBNORMAL 3
#define FP_NORMAL 4

#define fpclassify(x) \
  __builtin_fpclassify(FP_NAN, FP_INFINITE, FP_NORMAL, FP_SUBNORMAL, FP_ZERO, x)
#define isnan(x) __builtin_isnan(x)
#define isinf(x) __builtin_isinf(x)
#define isfinite(x) __builtin_isfinite(x)
#define isnormal(x) __builtin_isnormal(x)
#define signbit(x) __builtin_signbit(x)

#define M_E 2.7182818284590452354
#define M_LN2 0.69314718055994530942
#define M_LN10 2.30258509299404568402
#define M_LOG2E 1.4426950408889634074
#define M_LOG10E 0.43429448190325182765
#define M_PI 3.14159265358979323846
#define M_SQRT2 1.41421356237309504880
#define M_SQRT1_2 0.70710678118654752440

double acos(double);
double acosh(double);
double asin(double);
double asinh(double);
double atan(double);
double atan2(double, double);
double atanh(double);
double cbrt(double);
double ceil(double);
double copysign(double, double);
double cos(double);
double cosh(double);
double exp(double);
double exp2(double);
double expm1(double);
double fabs(double);
double fdim(double, double);
double floor(double);
double fma(double, double, double);
double fmax(double, double);
double fmin(double, double);
double fmod(double, double);
double frexp(double, int *);
double hypot(double, double);
double ldexp(double, int);
double log(double);
double log10(double);
double log1p(double);
double log2(double);
long lrint(double);
long long llrint(double);
long lround(double);
double modf(double, double *);
double nearbyint(double);
double nextafter(double, double);
double pow(double, double);
double remainder(double, double);
double rint(double);
double round(double);
double scalbn(double, int);
double sin(double);
double sinh(double);
double sqrt(double);
double tan(double);
double tanh(double);
double trunc(double);

float ceilf(float);
float cosf(float);
float fabsf(float);
float floorf(float);
float fmaxf(float, float);
float fminf(float, float);
float fmodf(float, float);
float powf(float, float);
float roundf(float);
float sinf(float);
float sqrtf(float);
float truncf(float);

#endif
