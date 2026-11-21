/* nookery_data.h — C ABI v1 for the Nookery data-folder adapter candidate.
 *
 * MIT, © 2026 the Nookery-RS contributors.  Companion implementation:
 * `rust/nookery-data-ffi`.
 *
 * STATUS OF THIS FILE
 * -------------------
 * This is an *adapter candidate*, not application integration.  The shipped
 * application still runs the C++ path in `pc/src/acgc_data.cpp`; nothing links
 * this ABI yet.  See `rust/nookery-data-ffi/README.md` for the exact split
 * between what is proven and what is not.
 *
 * OWNERSHIP RULES
 * ---------------
 * 1. Every pointer this library hands out is either an opaque handle owned by
 *    the caller until the matching `_destroy` call, or a `'static` NUL-
 *    terminated string that stays valid for the whole process lifetime
 *    (`nookery_data_status_name`, `nookery_data_status_message`).
 * 2. No function ever returns a pointer into Rust-owned mutable state.  Paths
 *    are copied into caller-provided buffers (`NookeryDataBuffer`).
 * 3. A handle is not safe for concurrent use.  One handle per thread.
 *
 * BUFFER RULES
 * ------------
 * `NookeryDataBuffer` is an in/out struct.  On entry the caller sets
 * `struct_size` to `sizeof(NookeryDataBuffer)`, `ptr` to writable storage (or
 * NULL when `capacity` is 0) and `capacity` to the number of bytes available.
 * On return:
 *   - `NOOKERY_DATA_STATUS_OK`: `length` bytes were written at `ptr`, plus one
 *     NUL terminator at `ptr[length]`.  `length` excludes the NUL.
 *   - `NOOKERY_DATA_STATUS_BUFFER_TOO_SMALL`: nothing was written.  `length` is
 *     the total size required *including* the NUL terminator, so the caller can
 *     retry with sufficient capacity.
 *   - any other status: `length` is 0 and `ptr` contents are unspecified.
 * A `struct_size` that does not match the library's `sizeof` is rejected with
 * `NOOKERY_DATA_STATUS_INVALID_ARGUMENT` without touching the buffer.
 *
 * VERSIONING
 * ----------
 * `NOOKERY_DATA_ABI_VERSION` is the layout version of every struct in this
 * file.  `NookeryDataCreateOptions.abi_version` must equal it; a mismatch is
 * rejected rather than reinterpreted.  Adding a field means bumping both the
 * version and the struct's `struct_size`.
 *
 * PANICS
 * ------
 * The crate is built with `panic = "abort"`, so there is no unwinding net:
 * every entry point validates all inputs and returns a status instead.  There
 * is deliberately no "panicked" status, because it could never be observed.
 */
#ifndef NOOKERY_DATA_H
#define NOOKERY_DATA_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NOOKERY_DATA_ABI_VERSION 1u

/* Outcome values.  0..12 mirror `nookery_core::ErrorCode` (upstream
 * `borealis::data::ErrorCode` declaration order) so a status can be compared
 * with the C++ oracle without translation.  13.. are ABI-level outcomes that
 * the C++ implementation expresses as a bare `false`/`0` return or as a `bool`
 * result, and that therefore have no upstream numeric value. */
#define NOOKERY_DATA_STATUS_OK 0u
#define NOOKERY_DATA_STATUS_NOT_INITIALIZED 1u
#define NOOKERY_DATA_STATUS_PREFERENCE_PATH_UNAVAILABLE 2u
#define NOOKERY_DATA_STATUS_EMPTY_PATH 3u
#define NOOKERY_DATA_STATUS_CREATE_DIRECTORY_FAILED 4u
#define NOOKERY_DATA_STATUS_NOT_DIRECTORY 5u
#define NOOKERY_DATA_STATUS_WRITE_PROBE_FAILED 6u
#define NOOKERY_DATA_STATUS_WRITE_PROBE_CLEANUP_FAILED 7u
#define NOOKERY_DATA_STATUS_DESCRIPTOR_WRITE_FAILED 8u
#define NOOKERY_DATA_STATUS_MIGRATION_INCOMPLETE 9u
#define NOOKERY_DATA_STATUS_OVERRIDE_ACTIVE 10u
#define NOOKERY_DATA_STATUS_UNSUPPORTED 11u
#define NOOKERY_DATA_STATUS_OPEN_FOLDER_FAILED 12u
/* ABI-level: null pointer, non-UTF-8 input, bad `struct_size`, bad ABI version,
 * non-absolute root, unknown enum/discriminant. */
#define NOOKERY_DATA_STATUS_INVALID_ARGUMENT 13u
/* ABI-level: output buffer too small; `length` holds the required size. */
#define NOOKERY_DATA_STATUS_BUFFER_TOO_SMALL 14u
/* ABI-level: relative path rejected (empty, absolute, rooted, or containing
 * `..`).  Upstream `pc_platform_user_path` joins blindly and would escape the
 * data root; this rejection is a deliberate hardening. */
#define NOOKERY_DATA_STATUS_PATH_REJECTED 15u
/* ABI-level: the handle was created without a base directory, so the upstream
 * `pc_platform_base_path` "not available" answer applies. */
#define NOOKERY_DATA_STATUS_BASE_PATH_UNAVAILABLE 16u
/* ABI-level: directory/plan index outside the valid range. */
#define NOOKERY_DATA_STATUS_INDEX_OUT_OF_RANGE 17u
/* ABI-level: allocation for an owned handle or plan failed. */
#define NOOKERY_DATA_STATUS_OUT_OF_MEMORY 18u

#define NOOKERY_DATA_STATUS_MAX 18u

typedef struct NookeryDataHandle NookeryDataHandle;
typedef struct NookeryDataPlan NookeryDataPlan;

/* In/out string buffer.  See BUFFER RULES above. */
typedef struct NookeryDataBuffer {
    uint32_t struct_size; /* in: sizeof(NookeryDataBuffer) */
    char *ptr;            /* in: writable storage or NULL */
    size_t capacity;      /* in: bytes available at ptr */
    size_t length;        /* out: see BUFFER RULES */
} NookeryDataBuffer;

/* Handle construction inputs.  All pointer fields are borrowed for the call
 * only; the library copies what it needs. */
typedef struct NookeryDataCreateOptions {
    uint32_t struct_size;         /* in: sizeof(NookeryDataCreateOptions) */
    uint32_t abi_version;         /* in: NOOKERY_DATA_ABI_VERSION */
    const char *root_utf8;        /* in: absolute active data directory */
    const char *base_root_utf8;   /* in: absolute install directory, or NULL */
} NookeryDataCreateOptions;

/* Result of `nookery_data_initialize`.  Failure is non-fatal by design: the
 * upstream `ensure_folders` logs a warning and keeps booting. */
typedef struct NookeryDataDirReport {
    uint32_t struct_size;    /* in: sizeof(NookeryDataDirReport) */
    uint32_t failure_count;  /* out: directories that could not be created */
} NookeryDataDirReport;

/* Selector for `nookery_data_directory`.  Declaration order is the order the
 * directories are created in: the four from `DataLayout::required_directories`,
 * then the cache directory that upstream `ensure_folders` appends.  The cache
 * location is `<root>/cache` by default and may be injected with
 * `nookery_data_set_cache_root`. */
typedef enum NookeryDataDirKind {
    NOOKERY_DATA_DIR_SAVE_CARD_A = 0,
    NOOKERY_DATA_DIR_SAVE_CARD_B = 1,
    NOOKERY_DATA_DIR_TEXTURE_PACK = 2,
    NOOKERY_DATA_DIR_NES_ROMS = 3,
    NOOKERY_DATA_DIR_CACHE = 4,
    NOOKERY_DATA_DIR_COUNT = 5
} NookeryDataDirKind;

/* --- Library identity ---------------------------------------------------- */
/* Layout version this library was built with. */
uint32_t nookery_data_abi_version(void);

/* --- Static text (process lifetime; never freed by the caller) ----------- */
/* Stable identifier such as "PATH_REJECTED"; never NULL, maybe "<unknown>". */
const char *nookery_data_status_name(uint32_t status);
/* The upstream `status_message` text for 0..12, and "" for
 * NOOKERY_DATA_STATUS_OK and for ABI-level statuses, which have no upstream
 * wording. */
const char *nookery_data_status_message(uint32_t status);
/* Count of defined statuses.  A caller can compare a status it was compiled
 * against with this value to detect a library that predates it. */
uint32_t nookery_data_status_count(void);

/* --- Handles ------------------------------------------------------------- */
/* Creates an owned handle.  Fails with INVALID_ARGUMENT when `options`,
 * `out_handle`, `options->root_utf8` is NULL/empty/non-UTF-8/not absolute, or
 * when `struct_size`/`abi_version` do not match.  On failure `*out_handle` is
 * set to NULL.  No filesystem access happens here. */
uint32_t nookery_data_handle_create(const NookeryDataCreateOptions *options,
                                    NookeryDataHandle **out_handle);
/* Frees a handle.  NULL is accepted and ignored. */
void nookery_data_handle_destroy(NookeryDataHandle *handle);

/* Overrides the cache directory used by initialization.  The default remains
 * <root>/cache; this setter lets a host inject Borealis' Paths::cachePath when
 * it is an external preference directory. */
uint32_t nookery_data_set_cache_root(NookeryDataHandle *handle,
                                     const char *cache_root_utf8);

/* Creates `save/card_a`, `save/card_b`, `texture_pack`, `nes_roms` and the
 * cache directory below the root, in that order, continuing past failures.
 * Idempotent: a second call succeeds and rewrites no existing content.
 * `out_report` may be NULL.  Returns OK when every directory is present, or the
 * first failure's status (the report still counts them all). */
uint32_t nookery_data_initialize(NookeryDataHandle *handle,
                                 NookeryDataDirReport *out_report);

/* Root and the individual directories by `NookeryDataDirKind`.  The selector is
 * the only enumeration order guarantee; `NOOKERY_DATA_DIR_COUNT` exists so a C
 * caller can size a loop without hardcoding 5. */
uint32_t nookery_data_root(NookeryDataHandle *handle, NookeryDataBuffer *out);
uint32_t nookery_data_directory(NookeryDataHandle *handle, uint32_t kind,
                                NookeryDataBuffer *out);

/* --- Path resolution (ports `pc_platform_user_path` / `_base_path`) ------ */
/* Joins `relative_utf8` below the active data root.  Rejects empty, absolute,
 * rooted and `..`-bearing inputs with PATH_REJECTED. */
uint32_t nookery_data_user_path(NookeryDataHandle *handle,
                                const char *relative_utf8,
                                NookeryDataBuffer *out);
/* Joins `relative_utf8` below the base root.  Returns BASE_PATH_UNAVAILABLE when
 * the handle has no base root, mirroring the upstream `pc_platform_base_path`
 * returning 0 on platforms without executable-directory discovery. */
uint32_t nookery_data_base_path(NookeryDataHandle *handle,
                                const char *relative_utf8,
                                NookeryDataBuffer *out);

/* --- Diagnostics --------------------------------------------------------- */
/* Status of the most recent operation on this handle (initially OK).  Reading
 * it, or `nookery_data_last_message`, never changes it. */
uint32_t nookery_data_last_status(const NookeryDataHandle *handle);
/* Rendered status text, including the offending path and OS error when known;
 * same composition as the upstream `status_message` helper. */
uint32_t nookery_data_last_message(NookeryDataHandle *handle,
                                   NookeryDataBuffer *out);

/* --- Migration planning (never copies) ---------------------------------- */
/* Plans entries present in `legacy_root_utf8` but absent from the active root.
 * Doing no filesystem mutation, this is the upstream `migration_rules` contract
 * minus the Borealis copy executor.  Directories come first, then named files,
 * then extension matches sorted by name. */
uint32_t nookery_data_plan_migration(NookeryDataHandle *handle,
                                     const char *legacy_root_utf8,
                                     NookeryDataPlan **out_plan);
uint32_t nookery_data_plan_counts(const NookeryDataPlan *plan,
                                  size_t *out_directories, size_t *out_files);
uint32_t nookery_data_plan_directory(const NookeryDataPlan *plan, size_t index,
                                     NookeryDataBuffer *out);
uint32_t nookery_data_plan_file(const NookeryDataPlan *plan, size_t index,
                                NookeryDataBuffer *out);
void nookery_data_plan_destroy(NookeryDataPlan *plan);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* NOOKERY_DATA_H */
