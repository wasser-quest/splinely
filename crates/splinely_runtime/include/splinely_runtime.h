#ifndef SPLINELY_RUNTIME_H
#define SPLINELY_RUNTIME_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

typedef struct SplinelyRuntimeHandle SplinelyRuntimeHandle;

/* ABI v1: byte-length UTF-8 JSON input (not necessarily NUL terminated), maximum
 * 64 MiB. Each create/request returns an owned NUL-terminated UTF-8 JSON string:
 * {"ok":true,"result":{"frame":...,"hostActions":[...]}} or
 * {"ok":false,"error":"..."}. Free every returned string, success or failure,
 * exactly once with splinely_runtime_result_free; never libc free, never modify it.
 * Returned strings remain valid independently of the runtime's lifetime.
 * create writes NULL to *out on failure and a new owned handle on success.
 * Use a fresh output slot; create does NOT free any preexisting handle in it.
 * Destroy each successful handle exactly once. Destroy/free NULL are no-ops.
 * Null inputs, invalid UTF-8/JSON, and unwinding Rust panics are diagnosed.
 * Allocation failure/process abort cannot be recovered.
 *
 * CALLER MEMORY CONTRACT: non-NULL input points to length readable bytes;
 * out points to a writable, aligned pointer slot, disjoint from input. A handle
 * must be live and produced by this library; never use it after destroy. No
 * concurrent operations (including destroy) on the same handle. Returned
 * strings must be live allocations from this library at free. Dangling, forged,
 * misaligned, undersized pointers, double free or data races are caller UB;
 * Rust panic guards cannot validate arbitrary native memory.
 * No rendering, sound synthesis, callbacks or customJs are executed here:
 * hostActions are requests for the caller to interpret, not executable code.
 */
char *splinely_runtime_create(const uint8_t *data, size_t length, SplinelyRuntimeHandle **out);
char *splinely_runtime_request(SplinelyRuntimeHandle *handle, const uint8_t *data, size_t length);
void splinely_runtime_destroy(SplinelyRuntimeHandle *handle);
void splinely_runtime_result_free(char *result);

#ifdef __cplusplus
}
#endif
#endif
