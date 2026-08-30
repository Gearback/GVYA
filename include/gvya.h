#ifndef GVYA_H
#define GVYA_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

typedef struct gvya_buffer {
    uint8_t *ptr;
    size_t len;
    size_t capacity;
} gvya_buffer;

/* Engine ABI v1 memory ownership:
 * - gvya_alloc() returns tracked scratch storage owned by GVYA; release that exact pointer once
 *   with gvya_dealloc(). The len argument is retained for ABI shape but is not trusted as allocation
 *   metadata.
 * - Successful output buffers are GVYA-owned. ptr/len/capacity are read-only descriptors for the
 *   caller. Do not mutate or independently free ptr. Release/reset the descriptor with
 *   gvya_buffer_free() before reuse. Fabricated or repeated ptr values are never passed to a raw
 *   allocator reconstruction path.
 * - Every gvya_buffer* argument itself must point to writable valid storage for the duration of the
 *   call. These rules harden ownership bookkeeping; they cannot make an invalid C pointer safe. */
uint32_t gvya_abi_version(void);
uint32_t gvya_pointer_width(void);
size_t gvya_buffer_struct_size(void);
uint8_t *gvya_alloc(size_t len);
void gvya_dealloc(uint8_t *ptr, size_t len);

/* Compiler operations consume the deterministic GVYASRC1 source-tree transport. They use the
 * same Engine ABI allocation/output functions as runtime operations. */
int32_t gvya_compiler_validate_source_tree(const uint8_t *archive, size_t archive_len, gvya_buffer *out);
int32_t gvya_compiler_build_source_tree(const uint8_t *archive, size_t archive_len, gvya_buffer *out);

/* Runtime handles are process-global inside this ABI and are bounded to 256 simultaneously open
 * runtimes. Hosts needing more should shard ownership across explicit processes/Engine instances. */
/* ABI v1: options is strict gvya.runtime.open-options/1 JSON. */
int32_t gvya_runtime_open_with_options_json(const uint8_t *artifact, size_t artifact_len, const uint8_t *options, size_t options_len, uint64_t *out_handle, gvya_buffer *out_message);
int32_t gvya_runtime_close(uint64_t handle);
int32_t gvya_runtime_info_json(uint64_t handle, gvya_buffer *out);
int32_t gvya_runtime_capabilities_json(uint64_t handle, gvya_buffer *out);
int32_t gvya_runtime_capability_info_json(uint64_t handle, const uint8_t *id, size_t id_len, gvya_buffer *out);
int32_t gvya_runtime_turn_json(uint64_t handle, const uint8_t *request, size_t request_len, gvya_buffer *out);
int32_t gvya_runtime_open_conversation_json(uint64_t handle, const uint8_t *request, size_t request_len, gvya_buffer *out);
int32_t gvya_runtime_capability_result_json(uint64_t handle, const uint8_t *request, size_t request_len, gvya_buffer *out);
int32_t gvya_runtime_asset_by_path(uint64_t handle, const uint8_t *path, size_t path_len, gvya_buffer *out);
int32_t gvya_runtime_asset_by_id(uint64_t handle, const uint8_t *id, size_t id_len, gvya_buffer *out);
int32_t gvya_runtime_asset_info_by_id_json(uint64_t handle, const uint8_t *id, size_t id_len, gvya_buffer *out);
void gvya_buffer_free(gvya_buffer *buffer);

#ifdef __cplusplus
}
#endif
#endif
