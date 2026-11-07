#ifndef FUJI_TIME_H
#define FUJI_TIME_H

#include <stddef.h>

/* Wall time derives from the board tick counter plus a fixed epoch, so a
 * run is deterministic for a given input sequence. */
typedef long long time_t;
typedef long clock_t;
typedef int clockid_t;

#define CLOCKS_PER_SEC 1000000L
#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1

struct timespec {
  time_t tv_sec;
  long tv_nsec;
};

struct tm {
  int tm_sec;
  int tm_min;
  int tm_hour;
  int tm_mday;
  int tm_mon;
  int tm_year;
  int tm_wday;
  int tm_yday;
  int tm_isdst;
  long tm_gmtoff;
  const char *tm_zone;
};

time_t time(time_t *out);
clock_t clock(void);
int clock_gettime(clockid_t clock, struct timespec *out);
struct tm *gmtime_r(const time_t *time, struct tm *out);
struct tm *localtime_r(const time_t *time, struct tm *out);
struct tm *gmtime(const time_t *time);
struct tm *localtime(const time_t *time);
time_t mktime(struct tm *tm);
time_t timegm(struct tm *tm);

#endif
