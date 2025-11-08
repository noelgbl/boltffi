#ifndef MOBIFFI_CORE_H
#define MOBIFFI_CORE_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#define VERSION_MAJOR 0

#define VERSION_MINOR 1

#define VERSION_PATCH 0

typedef struct PendingHandle PendingHandle;

typedef struct FfiBuf_i32 {
  int32_t *ptr;
  uintptr_t len;
  uintptr_t cap;
} FfiBuf_i32;

typedef struct FfiOption_i32 {
  bool isSome;
  int32_t value;
} FfiOption_i32;

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

typedef void *SubscriptionHandle;

typedef struct TestEvent {
  int32_t eventId;
  int64_t value;
} TestEvent;

#define PANIC_STATUS (FfiStatus){ .code = 10 }

void mffi_free_buf_i32(struct FfiBuf_i32 buf);

bool mffi_option_i32_is_some(struct FfiOption_i32 opt);

uint32_t mffi_version_major(void);

uint32_t mffi_version_minor(void);

uint32_t mffi_version_patch(void);

void mffi_free_buf_u8(struct FfiBuf_u8 buf);

void mffi_free_string(struct FfiString string);

struct FfiStatus mffi_last_error_message(struct FfiString *out);

void mffi_clear_last_error(void);

SubscriptionHandle mffi_test_events_subscribe(uintptr_t capacity);

bool mffi_test_events_push(SubscriptionHandle handle, int32_t event_id, int64_t value);

uintptr_t mffi_test_events_pop_batch(SubscriptionHandle handle,
                                     struct TestEvent *output_ptr,
                                     uintptr_t output_capacity);

int32_t mffi_test_events_wait(SubscriptionHandle handle, uint32_t timeout_milliseconds);

void mffi_test_events_unsubscribe(SubscriptionHandle handle);

void mffi_test_events_free(SubscriptionHandle handle);

void mffi_pending_cancel(struct PendingHandle *handle);

void mffi_pending_free(struct PendingHandle *handle);


/* Macro-generated types and exports */
typedef int32_t Direction;
#define Direction_North 0
#define Direction_East 1
#define Direction_South 2
#define Direction_West 3

typedef struct ApiResult {
  int32_t tag;
  union {
    int32_t ErrorCode;
    struct { int32_t code; int32_t detail; } ErrorWithData;
  } payload;
} ApiResult;
#define ApiResult_TAG_Success 0
#define ApiResult_TAG_ErrorCode 1
#define ApiResult_TAG_ErrorWithData 2

typedef int8_t StreamPollResult;
#define StreamPollResult_ItemsAvailable 0
#define StreamPollResult_Pending 1

typedef uint8_t ContinuationState;
#define ContinuationState_Empty 0
#define ContinuationState_Waked 1
#define ContinuationState_Stored 2
#define ContinuationState_Cancelled 3

typedef int32_t WaitResult;
#define WaitResult_EventsAvailable 1
#define WaitResult_Timeout 0
#define WaitResult_Unsubscribed -1

typedef int8_t RustFuturePoll;
#define RustFuturePoll_Ready 0
#define RustFuturePoll_MaybeReady 1

typedef uint8_t SchedulerStateTag;
#define SchedulerStateTag_Empty 0
#define SchedulerStateTag_Waked 1
#define SchedulerStateTag_Cancelled 2
#define SchedulerStateTag_ContinuationStored 3

typedef const void* RustFutureHandle;
typedef void (*RustFutureContinuationCallback)(uint64_t callback_data, RustFuturePoll poll_result);

typedef void (*StreamContinuationCallback)(uint64_t callback_data, int8_t poll_result);

typedef struct DataPoint {
  double x;
  double y;
  int64_t timestamp;
} DataPoint;

typedef struct SensorReading {
  int32_t sensorId;
  int64_t timestampMs;
  double value;
} SensorReading;

typedef struct DataProviderVTable {
  void (*free)(uint64_t handle);
  uint64_t (*clone)(uint64_t handle);
  void (*get_count)(uint64_t handle, uint32_t *out, struct FfiStatus *status);
  void (*get_item)(uint64_t handle, uint32_t index, DataPoint *out, struct FfiStatus *status);
} DataProviderVTable;

typedef struct ForeignDataProvider {
  const struct DataProviderVTable *vtable;
  uint64_t handle;
} ForeignDataProvider;

void mffi_register_data_provider_vtable(const struct DataProviderVTable *vtable);
struct ForeignDataProvider *mffi_create_data_provider(uint64_t handle);

typedef struct AsyncDataFetcherVTable {
  void (*free)(uint64_t handle);
  uint64_t (*clone)(uint64_t handle);
  void (*fetch_value)(uint64_t handle, uint32_t key, void (*callback)(uint64_t, uint64_t, struct FfiStatus), uint64_t callback_data);
} AsyncDataFetcherVTable;

typedef struct ForeignAsyncDataFetcher {
  const struct AsyncDataFetcherVTable *vtable;
  uint64_t handle;
} ForeignAsyncDataFetcher;

void mffi_register_async_data_fetcher_vtable(const struct AsyncDataFetcherVTable *vtable);
struct ForeignAsyncDataFetcher *mffi_create_async_data_fetcher(uint64_t handle);

struct FfiStatus mffi_greeting(const uint8_t* name_ptr, uintptr_t name_len, struct FfiString *out);
struct FfiStatus mffi_concat(const uint8_t* first_ptr, uintptr_t first_len, const uint8_t* second_ptr, uintptr_t second_len, struct FfiString *out);
struct FfiStatus mffi_reverse_string(const uint8_t* input_ptr, uintptr_t input_len, struct FfiString *out);
uintptr_t mffi_copy_bytes(const uint8_t* src_ptr, uintptr_t src_len, uint8_t* dst_ptr, uintptr_t dst_len);
struct Counter * mffi_counter_new(void);
struct FfiStatus mffi_counter_free(struct Counter * handle);
struct FfiStatus mffi_counter_set(struct Counter * handle, uint64_t value);
struct FfiStatus mffi_counter_increment(struct Counter * handle);
uint64_t mffi_counter_get(struct Counter * handle);
struct DataStore * mffi_datastore_new(void);
struct FfiStatus mffi_datastore_free(struct DataStore * handle);
struct FfiStatus mffi_datastore_add(struct DataStore * handle, DataPoint point);
uintptr_t mffi_datastore_len(struct DataStore * handle);
uintptr_t mffi_datastore_copy_into(struct DataStore * handle, DataPoint* dst_ptr, uintptr_t dst_len);
struct FfiStatus mffi_datastore_foreach(struct DataStore * handle, void (*callback_cb)(void*, DataPoint), void* callback_ud);
double mffi_datastore_sum(struct DataStore * handle);
int32_t mffi_add_numbers(int32_t first, int32_t second);
double mffi_multiply_floats(double first, double second);
struct FfiStatus mffi_make_greeting(const uint8_t* name_ptr, uintptr_t name_len, struct FfiString *out);
struct FfiStatus mffi_safe_divide(int32_t numerator, int32_t denominator, int32_t *out);
uintptr_t mffi_generate_sequence_len(int32_t count);
struct FfiStatus mffi_generate_sequence_copy_into(int32_t count, int32_t *dst, uintptr_t dst_cap, uintptr_t *written);
struct FfiStatus mffi_foreach_range(int32_t start, int32_t end, void (*callback_cb)(void*, int32_t), void* callback_ud);
struct Accumulator * mffi_accumulator_new(void);
struct FfiStatus mffi_accumulator_free(struct Accumulator * handle);
struct FfiStatus mffi_accumulator_add(struct Accumulator * handle, int64_t amount);
int64_t mffi_accumulator_get(struct Accumulator * handle);
struct FfiStatus mffi_accumulator_reset(struct Accumulator * handle);
Direction mffi_opposite_direction(Direction dir);
int32_t mffi_direction_to_degrees(Direction dir);
int32_t mffi_find_even(int32_t value, int32_t *out);
ApiResult mffi_process_value(int32_t value);
bool mffi_api_result_is_success(ApiResult result);
RustFutureHandle mffi_compute_heavy(int32_t input);
void mffi_compute_heavy_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
int32_t mffi_compute_heavy_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_compute_heavy_cancel(RustFutureHandle handle);
void mffi_compute_heavy_free(RustFutureHandle handle);
RustFutureHandle mffi_fetch_data(int32_t id);
void mffi_fetch_data_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
int32_t mffi_fetch_data_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_fetch_data_cancel(RustFutureHandle handle);
void mffi_fetch_data_free(RustFutureHandle handle);
RustFutureHandle mffi_async_make_string(int32_t value);
void mffi_async_make_string_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
struct FfiString mffi_async_make_string_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_async_make_string_cancel(RustFutureHandle handle);
void mffi_async_make_string_free(RustFutureHandle handle);
RustFutureHandle mffi_async_fetch_point(double x, double y);
void mffi_async_fetch_point_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
DataPoint mffi_async_fetch_point_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_async_fetch_point_cancel(RustFutureHandle handle);
void mffi_async_fetch_point_free(RustFutureHandle handle);
RustFutureHandle mffi_async_get_numbers(int32_t count);
void mffi_async_get_numbers_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
struct FfiBuf_i32 mffi_async_get_numbers_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_async_get_numbers_cancel(RustFutureHandle handle);
void mffi_async_get_numbers_free(RustFutureHandle handle);
RustFutureHandle mffi_async_find_value(int32_t needle);
void mffi_async_find_value_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
struct FfiOption_i32 mffi_async_find_value_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_async_find_value_cancel(RustFutureHandle handle);
void mffi_async_find_value_free(RustFutureHandle handle);
RustFutureHandle mffi_async_greeting(const uint8_t* name_ptr, uintptr_t name_len);
void mffi_async_greeting_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
struct FfiString mffi_async_greeting_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_async_greeting_cancel(RustFutureHandle handle);
void mffi_async_greeting_free(RustFutureHandle handle);
RustFutureHandle mffi_async_fetch_numbers(int32_t id);
void mffi_async_fetch_numbers_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);
struct FfiBuf_i32 mffi_async_fetch_numbers_complete(RustFutureHandle handle, struct FfiStatus* out_status);
void mffi_async_fetch_numbers_cancel(RustFutureHandle handle);
void mffi_async_fetch_numbers_free(RustFutureHandle handle);
struct SensorMonitor * mffi_sensormonitor_new(void);
struct FfiStatus mffi_sensormonitor_free(struct SensorMonitor * handle);
struct FfiStatus mffi_sensormonitor_emit_reading(struct SensorMonitor * handle, int32_t sensor_id, int64_t timestamp_ms, double value);
uintptr_t mffi_sensormonitor_subscriber_count(struct SensorMonitor * handle);
struct DataConsumer * mffi_dataconsumer_new(void);
struct FfiStatus mffi_dataconsumer_free(struct DataConsumer * handle);
struct FfiStatus mffi_dataconsumer_set_provider(struct DataConsumer * handle, struct ForeignDataProvider* provider);
uint64_t mffi_dataconsumer_compute_sum(struct DataConsumer * handle);
SubscriptionHandle mffi_sensormonitor_readings(const struct Sensormonitor *handle);
uintptr_t mffi_sensormonitor_readings_pop_batch(SubscriptionHandle subscription_handle, struct SensorReading *output_ptr, uintptr_t output_capacity);
int32_t mffi_sensormonitor_readings_wait(SubscriptionHandle subscription_handle, uint32_t timeout_milliseconds);
void mffi_sensormonitor_readings_poll(SubscriptionHandle subscription_handle, uint64_t callback_data, StreamContinuationCallback callback);
void mffi_sensormonitor_readings_unsubscribe(SubscriptionHandle subscription_handle);
void mffi_sensormonitor_readings_free(SubscriptionHandle subscription_handle);


#endif  /* MOBIFFI_CORE_H */
