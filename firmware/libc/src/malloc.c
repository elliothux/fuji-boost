/*
 * Deterministic heap over the 64 MiB arena at 0x10000000 (firmware layout).
 * Requests up to 32 KiB use segregated size classes with LIFO free lists;
 * larger ones are 4 KiB-granular blocks kept in an address-ordered free list
 * that splits on allocation and coalesces on free. Every block carries an
 * 8-byte header holding its usable size, so malloc_usable_size is exact and
 * user pointers are 8-byte aligned (ui-cabi's C_MALLOC_ALIGNMENT on 32-bit).
 */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define HEAP_BASE 0x10000000u
#define HEAP_SIZE 0x04000000u
#define LARGE_GRANULE 4096u
#define TAG_MAGIC 0xfb000000u
#define TAG_LARGE 0x00800000u
#define TAG_FREE 0x00400000u
#define TAG_CLASS_MASK 0x000000ffu

extern void fuji_mailbox_set(uint32_t word, uint32_t value);

typedef struct {
  uint32_t size;
  uint32_t tag;
} Header;

typedef struct FreeLarge {
  Header header;
  struct FreeLarge *next;
} FreeLarge;

static const uint32_t class_sizes[] = {
  16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536,
  2048, 3072, 4096, 6144, 8192, 12288, 16384, 24576, 32768,
};
#define CLASS_COUNT (sizeof(class_sizes) / sizeof(class_sizes[0]))
#define SMALL_LIMIT 32768u

static void *free_small[CLASS_COUNT];
static FreeLarge *free_large;
static uintptr_t top = HEAP_BASE;
static uint32_t used_bytes;
static uint32_t peak_bytes;
static uint32_t live_blocks;

static unsigned class_of(size_t size) {
  unsigned index = 0;
  while (class_sizes[index] < size) index++;
  return index;
}

static Header *header_of(void *pointer) {
  return (Header *)pointer - 1;
}

static void *bump(uint32_t total) {
  if (total > HEAP_BASE + HEAP_SIZE - top) return NULL;
  void *block = (void *)top;
  top += total;
  return block;
}

static void account(int32_t delta, int32_t blocks) {
  used_bytes += (uint32_t)delta;
  live_blocks += (uint32_t)blocks;
  if (used_bytes > peak_bytes) peak_bytes = used_bytes;
}

static void *large_alloc(size_t size) {
  uint32_t total = (uint32_t)((size + sizeof(Header) + LARGE_GRANULE - 1) & ~(size_t)(LARGE_GRANULE - 1));
  FreeLarge **link = &free_large;
  for (FreeLarge *block = free_large; block; link = &block->next, block = block->next) {
    uint32_t available = block->header.size + sizeof(Header);
    if (available < total) continue;
    if (available - total >= LARGE_GRANULE) {
      FreeLarge *rest = (FreeLarge *)((uint8_t *)block + total);
      rest->header.size = available - total - sizeof(Header);
      rest->header.tag = TAG_MAGIC | TAG_LARGE | TAG_FREE;
      rest->next = block->next;
      *link = rest;
    } else {
      total = available;
      *link = block->next;
    }
    block->header.size = total - sizeof(Header);
    block->header.tag = TAG_MAGIC | TAG_LARGE;
    return &block->header + 1;
  }
  Header *header = bump(total);
  if (!header) return NULL;
  header->size = total - sizeof(Header);
  header->tag = TAG_MAGIC | TAG_LARGE;
  return header + 1;
}

static void large_free(Header *header) {
  FreeLarge *block = (FreeLarge *)header;
  block->header.tag = TAG_MAGIC | TAG_LARGE | TAG_FREE;
  FreeLarge *previous = NULL;
  FreeLarge *next = free_large;
  while (next && next < block) {
    previous = next;
    next = next->next;
  }
  block->next = next;
  if (previous) previous->next = block;
  else free_large = block;
  if (next && (uint8_t *)block + block->header.size + sizeof(Header) == (uint8_t *)next) {
    block->header.size += next->header.size + sizeof(Header);
    block->next = next->next;
  }
  if (previous && (uint8_t *)previous + previous->header.size + sizeof(Header) == (uint8_t *)block) {
    previous->header.size += block->header.size + sizeof(Header);
    previous->next = block->next;
    block = previous;
  }
  /* A free block ending at the bump pointer is the list tail: give it back. */
  if ((uintptr_t)block + block->header.size + sizeof(Header) == top) {
    top = (uintptr_t)block;
    FreeLarge **link = &free_large;
    while (*link != block) link = &(*link)->next;
    *link = NULL;
  }
}

void *malloc(size_t size) {
  if (size == 0) size = 1;
  if (size > HEAP_SIZE) return NULL;
  void *pointer;
  if (size <= SMALL_LIMIT) {
    unsigned index = class_of(size);
    pointer = free_small[index];
    if (pointer) {
      free_small[index] = *(void **)pointer;
    } else {
      Header *header = bump(class_sizes[index] + sizeof(Header));
      if (!header) return NULL;
      header->size = class_sizes[index];
      header->tag = TAG_MAGIC | index;
      pointer = header + 1;
    }
    header_of(pointer)->tag &= ~TAG_FREE;
  } else {
    pointer = large_alloc(size);
    if (!pointer) return NULL;
  }
  account((int32_t)header_of(pointer)->size, 1);
  return pointer;
}

void free(void *pointer) {
  if (!pointer) return;
  Header *header = header_of(pointer);
  if ((header->tag & 0xff000000u) != TAG_MAGIC || (header->tag & TAG_FREE)) abort();
  account(-(int32_t)header->size, -1);
  if (header->tag & TAG_LARGE) {
    large_free(header);
    return;
  }
  header->tag |= TAG_FREE;
  unsigned index = header->tag & TAG_CLASS_MASK;
  *(void **)pointer = free_small[index];
  free_small[index] = pointer;
}

void *calloc(size_t count, size_t size) {
  if (size && count > (size_t)-1 / size) return NULL;
  void *pointer = malloc(count * size);
  if (pointer) memset(pointer, 0, count * size);
  return pointer;
}

void *realloc(void *pointer, size_t size) {
  if (!pointer) return malloc(size);
  if (size == 0) {
    free(pointer);
    return NULL;
  }
  Header *header = header_of(pointer);
  if (size <= header->size) return pointer;
  if ((header->tag & TAG_LARGE) && (uintptr_t)pointer + header->size == top) {
    uint32_t grow = (uint32_t)((size - header->size + LARGE_GRANULE - 1) & ~(size_t)(LARGE_GRANULE - 1));
    if (bump(grow)) {
      header->size += grow;
      account((int32_t)grow, 0);
      return pointer;
    }
  }
  void *moved = malloc(size);
  if (!moved) return NULL;
  memcpy(moved, pointer, header->size);
  free(pointer);
  return moved;
}

size_t malloc_usable_size(void *pointer) {
  return pointer ? header_of(pointer)->size : 0;
}

/* Mailbox words W_HEAP_USED, W_HEAP_PEAK, W_HEAP_BLOCKS. */
void fuji_heap_publish(void) {
  fuji_mailbox_set(11, used_bytes);
  fuji_mailbox_set(12, peak_bytes);
  fuji_mailbox_set(29, live_blocks);
}
