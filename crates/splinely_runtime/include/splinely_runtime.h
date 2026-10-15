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

/* Additive buffer API (same ABI v1; the four functions above are unchanged).
 * Avoids the per-call result allocation: the same {"ok":...} JSON envelope as
 * splinely_runtime_request is written to a caller buffer (UTF-8, NOT NUL
 * terminated; object key order is unspecified). *out_length always receives the
 * envelope size. Status codes:
 *   SPLINELY_STATUS_OK (0)               envelope copied; it may itself be {"ok":false,...}.
 *   SPLINELY_STATUS_BUFFER_TOO_SMALL (1) capacity < *out_length. The request HAS
 *       executed; the response is retained by the handle: fetch it with
 *       splinely_runtime_response_copy into a buffer of at least *out_length bytes.
 *       Do not resend the request.
 *   SPLINELY_STATUS_INVALID_ARGUMENT (2) NULL handle/out_length, or NULL out with
 *       capacity != 0; nothing was executed.
 * The retained response is replaced by the next _into call and dies with the
 * handle. Same CALLER MEMORY CONTRACT as above: out points to capacity writable
 * bytes (may be NULL when capacity is 0), out_length to a writable size_t, none
 * overlapping the input; no concurrent operations on one handle.
 *
 * request_into: like splinely_runtime_request (JSON request bytes).
 * tick_into:    {"type":"tick","delta":delta} without request JSON parsing.
 * response_copy: re-copy the retained response (also usable before any _into
 *               call: it is then empty and *out_length is 0). */
#define SPLINELY_STATUS_OK 0
#define SPLINELY_STATUS_BUFFER_TOO_SMALL 1
#define SPLINELY_STATUS_INVALID_ARGUMENT 2
int32_t splinely_runtime_request_into(SplinelyRuntimeHandle *handle, const uint8_t *data, size_t length,
                                      uint8_t *out, size_t capacity, size_t *out_length);
int32_t splinely_runtime_tick_into(SplinelyRuntimeHandle *handle, double delta,
                                   uint8_t *out, size_t capacity, size_t *out_length);
int32_t splinely_runtime_response_copy(SplinelyRuntimeHandle *handle,
                                       uint8_t *out, size_t capacity, size_t *out_length);

#ifdef __cplusplus
}
#endif
#endif
