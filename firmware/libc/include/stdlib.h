#ifndef FUJI_STDLIB_H
#define FUJI_STDLIB_H

#include <stddef.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *pointer, size_t size);
void free(void *pointer);
void abort(void) __attribute__((noreturn));
void exit(int status) __attribute__((noreturn));
char *getenv(const char *name);

int abs(int value);
long labs(long value);
long long llabs(long long value);
int atoi(const char *text);
long atol(const char *text);
long strtol(const char *text, char **end, int base);
unsigned long strtoul(const char *text, char **end, int base);
long long strtoll(const char *text, char **end, int base);
unsigned long long strtoull(const char *text, char **end, int base);
double strtod(const char *text, char **end);
double atof(const char *text);
void qsort(void *base, size_t count, size_t size, int (*compare)(const void *, const void *));

#define alloca __builtin_alloca

#endif
