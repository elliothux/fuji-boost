/* memcpy/memmove/memset/strlen come from compiler_builtins. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* compiler_builtins' weak memcmp/bcmp walk bytes; QuickJS atom/shape lookups
 * and ui-cabi style comparisons made that ~11% of a dashboard frame. */
__attribute__((no_builtin)) int memcmp(const void *left, const void *right, size_t length) {
  const unsigned char *a = left;
  const unsigned char *b = right;
  if ((((uintptr_t)a ^ (uintptr_t)b) & 3) == 0) {
    while (length && ((uintptr_t)a & 3)) {
      if (*a != *b) return *a - *b;
      a++, b++, length--;
    }
    while (length >= 4 && *(const uint32_t *)a == *(const uint32_t *)b) {
      a += 4, b += 4, length -= 4;
    }
  }
  for (; length; a++, b++, length--) {
    if (*a != *b) return *a - *b;
  }
  return 0;
}

__attribute__((no_builtin)) int bcmp(const void *left, const void *right, size_t length) {
  return memcmp(left, right, length);
}

void *memchr(const void *source, int value, size_t length) {
  const unsigned char *bytes = source;
  for (size_t index = 0; index < length; index++) {
    if (bytes[index] == (unsigned char)value) return (void *)(bytes + index);
  }
  return NULL;
}

size_t strnlen(const char *text, size_t limit) {
  size_t length = 0;
  while (length < limit && text[length]) length++;
  return length;
}

int strcmp(const char *left, const char *right) {
  while (*left && *left == *right) {
    left++;
    right++;
  }
  return (unsigned char)*left - (unsigned char)*right;
}

int strncmp(const char *left, const char *right, size_t limit) {
  for (; limit; limit--, left++, right++) {
    if (*left != *right || !*left) return (unsigned char)*left - (unsigned char)*right;
  }
  return 0;
}

char *strcpy(char *destination, const char *source) {
  char *out = destination;
  while ((*out++ = *source++)) {}
  return destination;
}

char *strncpy(char *destination, const char *source, size_t limit) {
  size_t index = 0;
  for (; index < limit && source[index]; index++) destination[index] = source[index];
  for (; index < limit; index++) destination[index] = '\0';
  return destination;
}

char *strcat(char *destination, const char *source) {
  strcpy(destination + strlen(destination), source);
  return destination;
}

char *strncat(char *destination, const char *source, size_t limit) {
  char *out = destination + strlen(destination);
  while (limit-- && *source) *out++ = *source++;
  *out = '\0';
  return destination;
}

char *strchr(const char *text, int value) {
  for (;; text++) {
    if (*text == (char)value) return (char *)text;
    if (!*text) return NULL;
  }
}

char *strrchr(const char *text, int value) {
  const char *found = NULL;
  for (;; text++) {
    if (*text == (char)value) found = text;
    if (!*text) return (char *)found;
  }
}

char *strstr(const char *haystack, const char *needle) {
  size_t length = strlen(needle);
  if (!length) return (char *)haystack;
  for (; *haystack; haystack++) {
    if (*haystack == *needle && !strncmp(haystack, needle, length)) return (char *)haystack;
  }
  return NULL;
}

size_t strspn(const char *text, const char *accept) {
  size_t length = 0;
  while (text[length] && strchr(accept, text[length])) length++;
  return length;
}

size_t strcspn(const char *text, const char *reject) {
  size_t length = 0;
  while (text[length] && !strchr(reject, text[length])) length++;
  return length;
}

char *strdup(const char *text) {
  size_t length = strlen(text) + 1;
  char *copy = malloc(length);
  if (copy) memcpy(copy, text, length);
  return copy;
}

char *strerror(int error) {
  (void)error;
  return (char *)"error";
}
