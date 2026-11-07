#ifndef FUJI_STDIO_H
#define FUJI_STDIO_H

#include <stdarg.h>
#include <stddef.h>

/* Every stream writes to the firmware log ring; there is no input. */
typedef struct FILE FILE;
extern FILE *const stdin;
extern FILE *const stdout;
extern FILE *const stderr;

#define EOF (-1)
#define BUFSIZ 256

int vsnprintf(char *buffer, size_t size, const char *format, va_list arguments);
int snprintf(char *buffer, size_t size, const char *format, ...)
  __attribute__((format(printf, 3, 4)));
int sprintf(char *buffer, const char *format, ...) __attribute__((format(printf, 2, 3)));
int vsprintf(char *buffer, const char *format, va_list arguments);
int vfprintf(FILE *stream, const char *format, va_list arguments);
int fprintf(FILE *stream, const char *format, ...) __attribute__((format(printf, 2, 3)));
int printf(const char *format, ...) __attribute__((format(printf, 1, 2)));
int vprintf(const char *format, va_list arguments);
int putchar(int c);
int puts(const char *text);
int fputs(const char *text, FILE *stream);
int fputc(int c, FILE *stream);
int putc(int c, FILE *stream);
size_t fwrite(const void *data, size_t size, size_t count, FILE *stream);
int fflush(FILE *stream);

#endif
