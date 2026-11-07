#undef assert
#ifdef NDEBUG
#define assert(expression) ((void)0)
#else
void __assert_fail(const char *expression, const char *file, int line, const char *function)
  __attribute__((noreturn));
#define assert(expression) \
  ((expression) ? (void)0 : __assert_fail(#expression, __FILE__, __LINE__, __func__))
#endif
#ifndef __cplusplus
#define static_assert _Static_assert
#endif
