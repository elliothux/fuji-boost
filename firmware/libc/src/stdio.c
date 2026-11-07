/*
 * printf family over the firmware log ring. Integer and string conversions
 * follow C99; %f/%e/%g use QuickJS's exact dtoa, so the exponent spelling
 * of %e/%g is JavaScript's ("1.5e+21"), which only debug output uses.
 */
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "dtoa.h"

extern void fuji_log_write(const char *bytes, size_t length);

struct FILE {
  int unused;
};
static struct FILE streams[3];
FILE *const stdin = &streams[0];
FILE *const stdout = &streams[1];
FILE *const stderr = &streams[2];

typedef struct {
  char *buffer;
  size_t size;
  size_t length;
} Sink;

static void emit(Sink *sink, char c) {
  if (sink->length + 1 < sink->size) sink->buffer[sink->length] = c;
  sink->length++;
}

static void emit_padded(Sink *sink, const char *text, size_t length, int width, int left, char pad) {
  int fill = width > (int)length ? width - (int)length : 0;
  if (!left && pad == '0' && length > 0 && (text[0] == '-' || text[0] == '+' || text[0] == ' ')) {
    emit(sink, *text++);
    length--;
  }
  if (!left) while (fill-- > 0) emit(sink, pad);
  for (size_t index = 0; index < length; index++) emit(sink, text[index]);
  if (left) while (fill-- > 0) emit(sink, ' ');
}

static size_t format_unsigned(char *out, unsigned long long value, unsigned base, int upper) {
  const char *digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
  char reversed[24];
  size_t count = 0;
  do {
    reversed[count++] = digits[value % base];
    value /= base;
  } while (value);
  for (size_t index = 0; index < count; index++) out[index] = reversed[count - 1 - index];
  return count;
}

int vsnprintf(char *buffer, size_t size, const char *format, va_list arguments) {
  Sink sink = {buffer, size, 0};
  for (const char *p = format; *p; p++) {
    if (*p != '%') {
      emit(&sink, *p);
      continue;
    }
    int left = 0, plus = 0, space = 0, alternate = 0, width = 0, precision = -1;
    char pad = ' ';
    for (;; p++) {
      if (p[1] == '-') left = 1;
      else if (p[1] == '0') pad = '0';
      else if (p[1] == '+') plus = 1;
      else if (p[1] == ' ') space = 1;
      else if (p[1] == '#') alternate = 1;
      else break;
    }
    p++;
    if (*p == '*') {
      width = va_arg(arguments, int);
      if (width < 0) { left = 1; width = -width; }
      p++;
    } else {
      while (*p >= '0' && *p <= '9') width = width * 10 + (*p++ - '0');
    }
    if (*p == '.') {
      p++;
      precision = 0;
      if (*p == '*') {
        precision = va_arg(arguments, int);
        p++;
      } else {
        while (*p >= '0' && *p <= '9') precision = precision * 10 + (*p++ - '0');
      }
    }
    int length = 0; /* 1 long, 2 long long, -1 short, -2 char, 3 size_t */
    if (*p == 'h') { length = -1; if (p[1] == 'h') { length = -2; p++; } p++; }
    else if (*p == 'l') { length = 1; if (p[1] == 'l') { length = 2; p++; } p++; }
    else if (*p == 'z' || *p == 't') { length = 3; p++; }
    else if (*p == 'j' || *p == 'q') { length = 2; p++; }
    if (left) pad = ' ';
    char text[128];
    size_t count = 0;
    switch (*p) {
      case 'd':
      case 'i': {
        long long value = length == 2 ? va_arg(arguments, long long)
          : length == 1 ? va_arg(arguments, long)
          : length == 3 ? (long long)va_arg(arguments, ptrdiff_t)
          : va_arg(arguments, int);
        if (length == -1) value = (short)value;
        if (length == -2) value = (signed char)value;
        unsigned long long magnitude = value < 0 ? 0ull - (unsigned long long)value : (unsigned long long)value;
        if (value < 0) text[count++] = '-';
        else if (plus) text[count++] = '+';
        else if (space) text[count++] = ' ';
        char digits[24];
        size_t digit_count = precision == 0 && magnitude == 0 ? 0 : format_unsigned(digits, magnitude, 10, 0);
        for (int zeros = precision - (int)digit_count; zeros > 0; zeros--) text[count++] = '0';
        memcpy(text + count, digits, digit_count);
        count += digit_count;
        emit_padded(&sink, text, count, width, left, precision >= 0 ? ' ' : pad);
        break;
      }
      case 'u':
      case 'x':
      case 'X':
      case 'o':
      case 'p': {
        unsigned long long value;
        unsigned base = *p == 'o' ? 8 : *p == 'u' ? 10 : 16;
        if (*p == 'p') {
          value = (uintptr_t)va_arg(arguments, void *);
          alternate = 1;
        } else {
          value = length == 2 ? va_arg(arguments, unsigned long long)
            : length == 1 ? va_arg(arguments, unsigned long)
            : length == 3 ? va_arg(arguments, size_t)
            : va_arg(arguments, unsigned int);
          if (length == -1) value = (unsigned short)value;
          if (length == -2) value = (unsigned char)value;
        }
        if (alternate && base == 16 && value) { text[count++] = '0'; text[count++] = *p == 'X' ? 'X' : 'x'; }
        if (alternate && base == 8) text[count++] = '0';
        char digits[24];
        size_t digit_count = precision == 0 && value == 0 ? 0 : format_unsigned(digits, value, base, *p == 'X');
        for (int zeros = precision - (int)digit_count; zeros > 0; zeros--) text[count++] = '0';
        memcpy(text + count, digits, digit_count);
        count += digit_count;
        emit_padded(&sink, text, count, width, left, precision >= 0 ? ' ' : pad);
        break;
      }
      case 'c':
        text[0] = (char)va_arg(arguments, int);
        emit_padded(&sink, text, 1, width, left, ' ');
        break;
      case 's': {
        const char *value = va_arg(arguments, const char *);
        if (!value) value = "(null)";
        size_t value_length = precision >= 0 ? strnlen(value, (size_t)precision) : strlen(value);
        emit_padded(&sink, value, value_length, width, left, ' ');
        break;
      }
      case 'f':
      case 'F':
      case 'e':
      case 'E':
      case 'g':
      case 'G': {
        double value = va_arg(arguments, double);
        JSDTOATempMem scratch;
        int digits = precision < 0 ? 6 : precision;
        int flags;
        if (*p == 'f' || *p == 'F') flags = JS_DTOA_FORMAT_FRAC | JS_DTOA_EXP_DISABLED;
        else if (*p == 'e' || *p == 'E') { flags = JS_DTOA_FORMAT_FIXED | JS_DTOA_EXP_ENABLED; digits += 1; }
        else { flags = JS_DTOA_FORMAT_FIXED | JS_DTOA_EXP_AUTO; if (digits == 0) digits = 1; }
        if (digits > JS_DTOA_MAX_DIGITS) digits = JS_DTOA_MAX_DIGITS;
        if (plus && !(value < 0)) text[count++] = '+';
        else if (space && !(value < 0)) text[count++] = ' ';
        if (js_dtoa_max_len(value, 10, digits, flags) + count + 1 < sizeof(text)) {
          count += (size_t)js_dtoa(text + count, value, 10, digits, flags, &scratch);
        }
        emit_padded(&sink, text, count, width, left, pad);
        break;
      }
      case 'n':
        *va_arg(arguments, int *) = (int)sink.length;
        break;
      case '%':
        emit(&sink, '%');
        break;
      default:
        emit(&sink, '%');
        if (*p) emit(&sink, *p);
        else p--;
        break;
    }
  }
  if (size) buffer[sink.length < size ? sink.length : size - 1] = '\0';
  return (int)sink.length;
}

int snprintf(char *buffer, size_t size, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vsnprintf(buffer, size, format, arguments);
  va_end(arguments);
  return length;
}

int vsprintf(char *buffer, const char *format, va_list arguments) {
  return vsnprintf(buffer, (size_t)-1 / 2, format, arguments);
}

int sprintf(char *buffer, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vsprintf(buffer, format, arguments);
  va_end(arguments);
  return length;
}

int vfprintf(FILE *stream, const char *format, va_list arguments) {
  char text[512];
  va_list copy;
  va_copy(copy, arguments);
  int length = vsnprintf(text, sizeof(text), format, copy);
  va_end(copy);
  (void)stream;
  fuji_log_write(text, length < (int)sizeof(text) ? (size_t)length : sizeof(text) - 1);
  return length;
}

int fprintf(FILE *stream, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vfprintf(stream, format, arguments);
  va_end(arguments);
  return length;
}

int vprintf(const char *format, va_list arguments) {
  return vfprintf(stdout, format, arguments);
}

int printf(const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vfprintf(stdout, format, arguments);
  va_end(arguments);
  return length;
}

int fputc(int c, FILE *stream) {
  char byte = (char)c;
  (void)stream;
  fuji_log_write(&byte, 1);
  return (unsigned char)c;
}

int putc(int c, FILE *stream) { return fputc(c, stream); }
int putchar(int c) { return fputc(c, stdout); }

int fputs(const char *text, FILE *stream) {
  (void)stream;
  fuji_log_write(text, strlen(text));
  return 0;
}

int puts(const char *text) {
  fputs(text, stdout);
  fputc('\n', stdout);
  return 0;
}

size_t fwrite(const void *data, size_t size, size_t count, FILE *stream) {
  (void)stream;
  fuji_log_write(data, size * count);
  return count;
}

int fflush(FILE *stream) {
  (void)stream;
  return 0;
}
