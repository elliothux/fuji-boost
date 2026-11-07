#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <sys/time.h>
#include <time.h>

extern uint64_t fuji_ticks(void);

#define TICKS_PER_SECOND 54000000ull
/* 2026-01-01T00:00:00Z: a fixed epoch keeps runs reproducible. */
#define EPOCH_SECONDS 1767225600ll

static void now(struct timespec *out, int realtime) {
  uint64_t ticks = fuji_ticks();
  out->tv_sec = (time_t)(ticks / TICKS_PER_SECOND) + (realtime ? EPOCH_SECONDS : 0);
  out->tv_nsec = (long)(ticks % TICKS_PER_SECOND * 1000 / 54);
}

int clock_gettime(clockid_t clock, struct timespec *out) {
  now(out, clock == CLOCK_REALTIME);
  return 0;
}

int gettimeofday(struct timeval *out, void *zone) {
  struct timespec spec;
  (void)zone;
  now(&spec, 1);
  out->tv_sec = spec.tv_sec;
  out->tv_usec = spec.tv_nsec / 1000;
  return 0;
}

time_t time(time_t *out) {
  struct timespec spec;
  now(&spec, 1);
  if (out) *out = spec.tv_sec;
  return spec.tv_sec;
}

clock_t clock(void) {
  return (clock_t)(fuji_ticks() / 54);
}

static int leap(long long year) {
  return (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
}

static const int month_days[2][12] = {
  {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31},
  {31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31},
};

struct tm *gmtime_r(const time_t *time, struct tm *out) {
  long long seconds = *time;
  long long days = seconds / 86400;
  long long rest = seconds % 86400;
  if (rest < 0) { rest += 86400; days--; }
  out->tm_hour = (int)(rest / 3600);
  out->tm_min = (int)(rest / 60 % 60);
  out->tm_sec = (int)(rest % 60);
  out->tm_wday = (int)(((days + 4) % 7 + 7) % 7);
  long long year = 1970;
  for (;;) {
    long long length = leap(year) ? 366 : 365;
    if (days >= 0 && days < length) break;
    if (days < 0) { year--; days += leap(year) ? 366 : 365; }
    else { days -= length; year++; }
  }
  out->tm_year = (int)(year - 1900);
  out->tm_yday = (int)days;
  int month = 0;
  while (days >= month_days[leap(year)][month]) days -= month_days[leap(year)][month++];
  out->tm_mon = month;
  out->tm_mday = (int)days + 1;
  out->tm_isdst = 0;
  out->tm_gmtoff = 0;
  out->tm_zone = "UTC";
  return out;
}

struct tm *localtime_r(const time_t *time, struct tm *out) {
  return gmtime_r(time, out);
}

static struct tm shared_tm;
struct tm *gmtime(const time_t *time) { return gmtime_r(time, &shared_tm); }
struct tm *localtime(const time_t *time) { return gmtime_r(time, &shared_tm); }

time_t timegm(struct tm *tm) {
  long long year = tm->tm_year + 1900ll + tm->tm_mon / 12;
  int month = tm->tm_mon % 12;
  if (month < 0) { month += 12; year--; }
  long long days = 0;
  for (long long y = 1970; y < year; y++) days += leap(y) ? 366 : 365;
  for (long long y = year; y < 1970; y++) days -= leap(y) ? 366 : 365;
  for (int m = 0; m < month; m++) days += month_days[leap(year)][m];
  days += tm->tm_mday - 1;
  time_t result = days * 86400 + tm->tm_hour * 3600ll + tm->tm_min * 60ll + tm->tm_sec;
  gmtime_r(&result, tm);
  return result;
}

time_t mktime(struct tm *tm) {
  return timegm(tm);
}

int pthread_mutex_init(pthread_mutex_t *mutex, const pthread_mutexattr_t *attributes) {
  (void)attributes;
  mutex->locked = 0;
  return 0;
}
int pthread_mutex_destroy(pthread_mutex_t *mutex) { (void)mutex; return 0; }
int pthread_mutex_lock(pthread_mutex_t *mutex) { mutex->locked++; return 0; }
int pthread_mutex_unlock(pthread_mutex_t *mutex) { mutex->locked--; return 0; }
int pthread_cond_init(pthread_cond_t *cond, const pthread_condattr_t *attributes) {
  (void)cond;
  (void)attributes;
  return 0;
}
int pthread_cond_destroy(pthread_cond_t *cond) { (void)cond; return 0; }
int pthread_cond_signal(pthread_cond_t *cond) { (void)cond; return 0; }
int pthread_cond_broadcast(pthread_cond_t *cond) { (void)cond; return 0; }
/* No other thread can signal: a wait would never end, so it fails. */
int pthread_cond_wait(pthread_cond_t *cond, pthread_mutex_t *mutex) {
  (void)cond;
  (void)mutex;
  return EINVAL;
}
int pthread_cond_timedwait(pthread_cond_t *cond, pthread_mutex_t *mutex, const struct timespec *at) {
  (void)cond;
  (void)mutex;
  (void)at;
  return ETIMEDOUT;
}
int pthread_condattr_init(pthread_condattr_t *attributes) { (void)attributes; return 0; }
int pthread_condattr_destroy(pthread_condattr_t *attributes) { (void)attributes; return 0; }
int pthread_condattr_setclock(pthread_condattr_t *attributes, int clock) {
  (void)attributes;
  (void)clock;
  return 0;
}

int fegetround(void) {
  uint32_t fpscr;
  __asm__ volatile("vmrs %0, fpscr" : "=r"(fpscr));
  return (int)(fpscr & 0xc00000u);
}

int fesetround(int mode) {
  uint32_t fpscr;
  if (mode & ~0xc00000) return 1;
  __asm__ volatile("vmrs %0, fpscr" : "=r"(fpscr));
  fpscr = (fpscr & ~0xc00000u) | (uint32_t)mode;
  __asm__ volatile("vmsr fpscr, %0" : : "r"(fpscr));
  return 0;
}
