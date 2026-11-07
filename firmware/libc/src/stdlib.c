#include <ctype.h>
#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "dtoa.h"

extern void fuji_abort(const char *reason) __attribute__((noreturn));

int errno;

void abort(void) {
  fuji_abort("abort()");
}

void exit(int status) {
  (void)status;
  fuji_abort("exit()");
}

void __assert_fail(const char *expression, const char *file, int line, const char *function) {
  fprintf(stderr, "assertion failed: %s (%s:%d %s)\n", expression, file, line, function);
  fuji_abort("assert");
}

char *getenv(const char *name) {
  (void)name;
  return NULL;
}

int abs(int value) { return value < 0 ? -value : value; }
long labs(long value) { return value < 0 ? -value : value; }
long long llabs(long long value) { return value < 0 ? -value : value; }

static unsigned long long parse_unsigned(const char *text, char **end, int base, int *negative, int *overflow) {
  const char *p = text;
  while (isspace((unsigned char)*p)) p++;
  *negative = 0;
  if (*p == '-' || *p == '+') *negative = *p++ == '-';
  if ((base == 0 || base == 16) && p[0] == '0' && (p[1] | 32) == 'x' && isxdigit((unsigned char)p[2])) {
    p += 2;
    base = 16;
  } else if (base == 0) {
    base = *p == '0' ? 8 : 10;
  }
  unsigned long long value = 0;
  const char *digits = p;
  *overflow = 0;
  for (;; p++) {
    int digit = isdigit((unsigned char)*p) ? *p - '0'
      : isalpha((unsigned char)*p) ? (*p | 32) - 'a' + 10 : 99;
    if (digit >= base) break;
    if (value > (ULLONG_MAX - (unsigned)digit) / (unsigned)base) *overflow = 1;
    value = value * (unsigned)base + (unsigned)digit;
  }
  if (end) *end = (char *)(p == digits ? text : p);
  return value;
}

unsigned long long strtoull(const char *text, char **end, int base) {
  int negative, overflow;
  unsigned long long value = parse_unsigned(text, end, base, &negative, &overflow);
  if (overflow) { errno = ERANGE; return ULLONG_MAX; }
  return negative ? 0ull - value : value;
}

long long strtoll(const char *text, char **end, int base) {
  int negative, overflow;
  unsigned long long value = parse_unsigned(text, end, base, &negative, &overflow);
  if (overflow || value > (unsigned long long)LLONG_MAX + negative) {
    errno = ERANGE;
    return negative ? LLONG_MIN : LLONG_MAX;
  }
  return negative ? (long long)(0ull - value) : (long long)value;
}

unsigned long strtoul(const char *text, char **end, int base) {
  unsigned long long value = strtoull(text, end, base);
  if (value > ULONG_MAX && value < 0ull - ULONG_MAX) { errno = ERANGE; return ULONG_MAX; }
  return (unsigned long)value;
}

long strtol(const char *text, char **end, int base) {
  long long value = strtoll(text, end, base);
  if (value > LONG_MAX) { errno = ERANGE; return LONG_MAX; }
  if (value < LONG_MIN) { errno = ERANGE; return LONG_MIN; }
  return (long)value;
}

int atoi(const char *text) { return (int)strtol(text, NULL, 10); }
long atol(const char *text) { return strtol(text, NULL, 10); }

double strtod(const char *text, char **end) {
  JSATODTempMem scratch;
  const char *next;
  while (isspace((unsigned char)*text)) text++;
  double value = js_atod(text, &next, 10, 0, &scratch);
  if (end) *end = (char *)next;
  return value;
}

double atof(const char *text) { return strtod(text, NULL); }

static void swap_bytes(unsigned char *left, unsigned char *right, size_t size) {
  while (size--) {
    unsigned char byte = *left;
    *left++ = *right;
    *right++ = byte;
  }
}

/* Insertion sort: callers here sort a handful of elements. */
void qsort(void *base, size_t count, size_t size, int (*compare)(const void *, const void *)) {
  unsigned char *bytes = base;
  for (size_t index = 1; index < count; index++) {
    for (size_t at = index; at > 0 && compare(bytes + (at - 1) * size, bytes + at * size) > 0; at--) {
      swap_bytes(bytes + (at - 1) * size, bytes + at * size, size);
    }
  }
}
