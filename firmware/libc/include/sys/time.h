#ifndef FUJI_SYS_TIME_H
#define FUJI_SYS_TIME_H

#include <time.h>

typedef long suseconds_t;

struct timeval {
  time_t tv_sec;
  suseconds_t tv_usec;
};

struct timezone {
  int tz_minuteswest;
  int tz_dsttime;
};

int gettimeofday(struct timeval *out, void *zone);

#endif
