#ifndef MOBIFFI_CORE_H
#define MOBIFFI_CORE_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#define VERSION_MAJOR 0

#define VERSION_MINOR 1

#define VERSION_PATCH 0

typedef struct Counter Counter;

typedef struct DataStore DataStore;

typedef struct FfiBuf_u8 {
  uint8_t *ptr;
  uintptr_t len;
  uintptr_t cap;
} FfiBuf_u8;

typedef struct FfiString {
  uint8_t *ptr;
  uintptr_t len;
  uintptr_t cap;
} FfiString;

typedef struct FfiStatus {
  int32_t code;
} FfiStatus;
#define FfiStatus_OK (FfiStatus){ .code = 0 }
#define FfiStatus_NULL_POINTER (FfiStatus){ .code = 1 }
#define FfiStatus_BUFFER_TOO_SMALL (FfiStatus){ .code = 2 }
#define FfiStatus_INVALID_ARG (FfiStatus){ .code = 3 }
#define FfiStatus_CANCELLED (FfiStatus){ .code = 4 }
#define FfiStatus_INTERNAL_ERROR (FfiStatus){ .code = 100 }

typedef struct DataPoint {
  double x;
  double y;
  int64_t timestamp;
} DataPoint;

typedef void (*DataPointCallback)(void *user_data, struct DataPoint point);

typedef void (*SumCallback)(void *user_data, double sum);

#define PANIC_STATUS (FfiStatus){ .code = 10 }

uint32_t mffi_version_major(void);

uint32_t mffi_version_minor(void);

uint32_t mffi_version_patch(void);

void mffi_free_buf_u8(struct FfiBuf_u8 buf);

void mffi_free_string(struct FfiString string);

struct FfiStatus mffi_last_error_message(struct FfiString *out);

void mffi_clear_last_error(void);

struct FfiStatus mffi_greeting(const uint8_t *name_ptr, uintptr_t name_len, struct FfiString *out);

struct FfiStatus mffi_concat(const uint8_t *first_ptr,
                             uintptr_t first_len,
                             const uint8_t *second_ptr,
                             uintptr_t second_len,
                             struct FfiString *out);

struct FfiStatus mffi_copy_bytes(const uint8_t *src,
                                 uintptr_t src_len,
                                 uint8_t *dst,
                                 uintptr_t dst_cap,
                                 uintptr_t *written);

struct Counter *mffi_counter_new(uint64_t initial);

struct FfiStatus mffi_counter_increment(struct Counter *handle);

struct FfiStatus mffi_counter_get(struct Counter *handle, uint64_t *out);

void mffi_counter_free(struct Counter *handle);

struct DataStore *mffi_datastore_new(void);

struct FfiStatus mffi_datastore_add(struct DataStore *handle, struct DataPoint point);

uintptr_t mffi_datastore_len(struct DataStore *handle);

struct FfiStatus mffi_datastore_copy_into(struct DataStore *handle,
                                          struct DataPoint *dst,
                                          uintptr_t dst_cap,
                                          uintptr_t *written);

void mffi_datastore_free(struct DataStore *handle);

struct FfiStatus mffi_datastore_foreach(struct DataStore *handle,
                                        DataPointCallback callback,
                                        void *user_data);

struct FfiStatus mffi_datastore_sum_async(struct DataStore *handle,
                                          SumCallback callback,
                                          void *user_data);


/* Macro-generated exports */
int32_t mffi_add_numbers(int32_t first, int32_t second);
double mffi_multiply_floats(double first, double second);
struct FfiStatus mffi_make_greeting(const uint8_t* name_ptr, uintptr_t name_len, struct FfiString *out);
struct FfiStatus mffi_safe_divide(int32_t numerator, int32_t denominator, int32_t *out);

#endif  /* MOBIFFI_CORE_H */
