/* harness.c — deterministic C-facing exercise of the nookery data ABI.
 *
 * Why this exists: Rust unit tests call the ABI through Rust types, so they
 * cannot catch a header that lies (wrong struct_size, wrong field order, wrong
 * status number) or a symbol that is missing from the static library.  This
 * file is compiled by a real C compiler against `include/nookery_data.h` and
 * linked against the real artifact.
 *
 * Expectations come from the header macros (`NOOKERY_DATA_STATUS_*`) and from
 * values the library reports at runtime — never from hardcoded numbers.  String
 * assertions compare against `nookery_data_status_message` only to prove the
 * two accessors agree; the byte-exact upstream wording is pinned in the Rust
 * tests, which can see both tables.
 *
 * Every case inspects returned values (status code, `length`, buffer contents,
 * which directories exist).  A nonzero exit alone is not evidence.
 *
 * Build & run: `make -C tools/data-ffi run`
 */
#include "nookery_data.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static int failures;
static int checks;

#define CHECK(cond)                                                            \
    do {                                                                       \
        ++checks;                                                              \
        if (!(cond)) {                                                         \
            ++failures;                                                        \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);    \
        }                                                                      \
    } while (0)

#define CHECK_STATUS(expected, actual)                                         \
    do {                                                                       \
        uint32_t expected_ = (expected);                                       \
        uint32_t actual_ = (actual);                                           \
        ++checks;                                                              \
        if (expected_ != actual_) {                                            \
            ++failures;                                                        \
            fprintf(stderr, "FAIL %s:%d: expected %s (%u), got %s (%u)\n",     \
                    __FILE__, __LINE__, nookery_data_status_name(expected_),   \
                    (unsigned)expected_, nookery_data_status_name(actual_),    \
                    (unsigned)actual_);                                        \
        }                                                                      \
    } while (0)

#define SENTINEL 0x7f

/* Caller storage with a poison byte so "nothing was written" is observable. */
typedef struct {
    char bytes[1024];
    size_t capacity;
} Out;

static void out_init(Out *out, size_t capacity) {
    memset(out->bytes, SENTINEL, sizeof out->bytes);
    out->capacity = capacity;
}

static NookeryDataBuffer out_buffer(Out *out) {
    NookeryDataBuffer buffer;
    buffer.struct_size = (uint32_t)sizeof(NookeryDataBuffer);
    buffer.ptr = out->bytes;
    buffer.capacity = out->capacity;
    buffer.length = 0;
    return buffer;
}

static int out_untouched(const Out *out) {
    for (size_t i = 0; i < out->capacity; ++i) {
        if ((unsigned char)out->bytes[i] != SENTINEL) return 0;
    }
    return 1;
}

/* Recursive mkdir -p for the temp root we own. */
static int make_dirs(const char *path) {
    char scratch[512];
    size_t length = strlen(path);
    if (length >= sizeof scratch) return -1;
    memcpy(scratch, path, length + 1);
    for (size_t i = 1; i <= length; ++i) {
        if (scratch[i] == '/' || scratch[i] == '\0') {
            char saved = scratch[i];
            scratch[i] = '\0';
            if (mkdir(scratch, 0700) != 0 && errno != EEXIST) return -1;
            scratch[i] = saved;
        }
    }
    return 0;
}

/* Build "<prefix>/<suffix>" into caller storage.  Taking the prefix as a plain
 * pointer (not a known-size array) keeps the compiler from emitting
 * format-truncation diagnostics for paths whose real length is bounded by the
 * pid-based temp root; the return value still reports the truncation case. */
static int join_path(char *destination, size_t capacity, const char *prefix,
                     const char *suffix) {
    int written = snprintf(destination, capacity, "%s/%s", prefix, suffix);
    return written >= 0 && (size_t)written < capacity;
}

/* "<prefix><suffix>" with a plain pointer prefix, for the same reason as
 * join_path. */
static int append_suffix(char *destination, size_t capacity, const char *prefix,
                         const char *suffix) {
    int written = snprintf(destination, capacity, "%s%s", prefix, suffix);
    return written >= 0 && (size_t)written < capacity;
}

static int is_dir(const char *path) {
    struct stat info;
    return stat(path, &info) == 0 && S_ISDIR(info.st_mode);
}

static int write_file(const char *path, const char *contents) {
    FILE *file = fopen(path, "wb");
    if (file == NULL) return -1;
    size_t length = strlen(contents);
    int ok = fwrite(contents, 1, length, file) == length;
    fclose(file);
    return ok ? 0 : -1;
}

static int file_equals(const char *path, const char *contents) {
    FILE *file = fopen(path, "rb");
    if (file == NULL) return 0;
    char buffer[64] = { 0 };
    size_t got = fread(buffer, 1, sizeof buffer - 1, file);
    fclose(file);
    return got == strlen(contents) && memcmp(buffer, contents, got) == 0;
}

static NookeryDataHandle *make_handle(const char *root, const char *base) {
    NookeryDataCreateOptions options;
    options.struct_size = (uint32_t)sizeof options;
    options.abi_version = nookery_data_abi_version();
    options.root_utf8 = root;
    options.base_root_utf8 = base;
    NookeryDataHandle *handle = NULL;
    uint32_t status = nookery_data_handle_create(&options, &handle);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, status);
    CHECK(handle != NULL);
    return handle;
}

/* --- cases --------------------------------------------------------------- */

static void test_identity(void) {
    CHECK(nookery_data_abi_version() == NOOKERY_DATA_ABI_VERSION);
    CHECK(nookery_data_status_count() > 0);

    /* Every status the header names must round-trip through its name accessor
     * and yield the message accessor's answer without crashing. */
    const uint32_t statuses[] = {
        NOOKERY_DATA_STATUS_OK,
        NOOKERY_DATA_STATUS_EMPTY_PATH,
        NOOKERY_DATA_STATUS_NOT_DIRECTORY,
        NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
        NOOKERY_DATA_STATUS_BUFFER_TOO_SMALL,
        NOOKERY_DATA_STATUS_PATH_REJECTED,
        NOOKERY_DATA_STATUS_INDEX_OUT_OF_RANGE,
        NOOKERY_DATA_STATUS_MAX,
    };
    for (size_t i = 0; i < sizeof statuses / sizeof statuses[0]; ++i) {
        const char *name = nookery_data_status_name(statuses[i]);
        const char *message = nookery_data_status_message(statuses[i]);
        CHECK(name != NULL && name[0] != '\0');
        CHECK(message != NULL);
        if (statuses[i] == NOOKERY_DATA_STATUS_OK) {
            CHECK(message[0] == '\0');
        }
        if (statuses[i] == NOOKERY_DATA_STATUS_PATH_REJECTED) {
            CHECK(message[0] == '\0'); /* ABI-only: no upstream wording */
            CHECK(strcmp(name, "PATH_REJECTED") == 0);
        }
    }
    CHECK(strcmp(nookery_data_status_name(NOOKERY_DATA_STATUS_MAX + 1), "<unknown>") == 0);
}

static void test_create_rejections(const char *root) {
    NookeryDataHandle *handle = (NookeryDataHandle *)&handle; /* poison */
    NookeryDataCreateOptions options;

    /* Null options. */
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_handle_create(NULL, &handle));
    CHECK(handle == NULL);

    /* Null out-pointer. */
    options.struct_size = (uint32_t)sizeof options;
    options.abi_version = nookery_data_abi_version();
    options.root_utf8 = root;
    options.base_root_utf8 = NULL;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_handle_create(&options, NULL));

    /* Wrong struct_size. */
    options.struct_size = 0;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_handle_create(&options, &handle));
    CHECK(handle == NULL);

    /* Wrong ABI version. */
    options.struct_size = (uint32_t)sizeof options;
    options.abi_version = NOOKERY_DATA_ABI_VERSION + 1;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_handle_create(&options, &handle));
    CHECK(handle == NULL);

    /* Null root. */
    options.abi_version = NOOKERY_DATA_ABI_VERSION;
    options.root_utf8 = NULL;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_handle_create(&options, &handle));

    /* Relative root, empty root, escaping root. */
    const char *bad_roots[] = { "relative", "", "/data/../etc" };
    for (size_t i = 0; i < sizeof bad_roots / sizeof bad_roots[0]; ++i) {
        options.root_utf8 = bad_roots[i];
        CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                     nookery_data_handle_create(&options, &handle));
        CHECK(handle == NULL);
    }

    /* Non-UTF-8 root bytes. */
    char broken[1024];
    size_t length = strlen(root);
    memcpy(broken, root, length);
    broken[length] = (char)0xff;
    broken[length + 1] = '\0';
    options.root_utf8 = broken;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_handle_create(&options, &handle));

    /* Destroying NULL is legal. */
    nookery_data_handle_destroy(NULL);
}

static void test_success_and_repeat(const char *root) {
    NookeryDataHandle *handle = make_handle(root, NULL);

    /* Nothing is created by handle creation alone. */
    CHECK(!is_dir(root));

    NookeryDataDirReport report;
    report.struct_size = (uint32_t)sizeof report;
    report.failure_count = 0xffffffffu;
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_initialize(handle, &report));
    CHECK(report.failure_count == 0);

    CHECK(is_dir(root));
    const char *required[] = { "save/card_a", "save/card_b", "texture_pack", "nes_roms", "cache" };
    for (size_t i = 0; i < sizeof required / sizeof required[0]; ++i) {
        char path[1024];
        if (!join_path(path, sizeof path, root, required[i])) {
            CHECK(0 && "required path too long");
            continue;
        }
        CHECK(is_dir(path));
    }

    /* Enumerated directories must be real, and reported in upstream order. */
    const char *expected_suffix[] = {
        "/save/card_a", "/save/card_b", "/texture_pack", "/nes_roms", "/cache"
    };
    for (uint32_t kind = 0; kind < NOOKERY_DATA_DIR_COUNT; ++kind) {
        Out out;
        out_init(&out, sizeof out.bytes);
        NookeryDataBuffer buffer = out_buffer(&out);
        CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_directory(handle, kind, &buffer));
        CHECK(buffer.length == strlen(out.bytes));
        CHECK(strlen(out.bytes) > strlen(root));
        CHECK(strcmp(out.bytes + strlen(out.bytes) - strlen(expected_suffix[kind]),
                     expected_suffix[kind]) == 0);
        CHECK(is_dir(out.bytes));
    }

    /* The root accessor returns the exact root text. */
    Out root_out;
    out_init(&root_out, sizeof root_out.bytes);
    NookeryDataBuffer root_buffer = out_buffer(&root_out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_root(handle, &root_buffer));
    CHECK(strcmp(root_out.bytes, root) == 0);
    CHECK(root_buffer.length == strlen(root));

    /* Repeated initialization: success, and existing content is preserved. */
    char marker[1024];
    CHECK(join_path(marker, sizeof marker, root, "save/card_a/town.gci"));
    CHECK(write_file(marker, "save") == 0);

    report.failure_count = 0xffffffffu;
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_initialize(handle, &report));
    CHECK(report.failure_count == 0);
    CHECK(file_equals(marker, "save"));

    /* The reported diagnostic for a successful call is the empty message. */
    Out message;
    out_init(&message, sizeof message.bytes);
    NookeryDataBuffer message_buffer = out_buffer(&message);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_last_message(handle, &message_buffer));
    CHECK(message_buffer.length == 0);
    CHECK(message.bytes[0] == '\0');
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_last_status(handle));

    nookery_data_handle_destroy(handle);
}

static void test_missing_root(const char *root) {
    /* A root that does not exist yet is a valid target: creating it is exactly
     * what initialization is for. */
    char missing[1024];
    CHECK(append_suffix(missing, sizeof missing, root, "/missing"));
    NookeryDataHandle *handle = make_handle(missing, NULL);
    CHECK(!is_dir(missing));
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_initialize(handle, NULL));
    CHECK(is_dir(missing));
    nookery_data_handle_destroy(handle);
}

static void test_null_arguments(const char *root) {
    NookeryDataHandle *handle = make_handle(root, NULL);
    Out out;
    NookeryDataBuffer buffer;

    /* Null handle. */
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_initialize(NULL, NULL));
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT, nookery_data_root(NULL, &buffer));
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_directory(NULL, NOOKERY_DATA_DIR_SAVE_CARD_A, &buffer));
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_user_path(NULL, "save", &buffer));
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_last_status(NULL));
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_last_message(NULL, &buffer));

    /* Null out-struct. */
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT, nookery_data_root(handle, NULL));
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_user_path(handle, "save", NULL));

    /* Null relative path. */
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_user_path(handle, NULL, &buffer));
    CHECK(out_untouched(&out));

    /* Null buffer pointer with nonzero capacity. */
    buffer = out_buffer(&out);
    buffer.ptr = NULL;
    buffer.capacity = 16;
    buffer.length = 99;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT, nookery_data_root(handle, &buffer));
    CHECK(buffer.length == 0);

    /* Wrong buffer struct_size. */
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    buffer.struct_size = (uint32_t)sizeof(NookeryDataBuffer) - 4;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT, nookery_data_root(handle, &buffer));
    CHECK(out_untouched(&out));

    /* Out-of-range directory selector. */
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_INDEX_OUT_OF_RANGE,
                 nookery_data_directory(handle, NOOKERY_DATA_DIR_COUNT, &buffer));
    CHECK(buffer.length == 0);
    CHECK(out_untouched(&out));

    nookery_data_handle_destroy(handle);
}

static void test_undersized_output(const char *root) {
    NookeryDataHandle *handle = make_handle(root, NULL);

    /* Zero capacity: report the requirement, write nothing. */
    NookeryDataBuffer probe;
    probe.struct_size = (uint32_t)sizeof probe;
    probe.ptr = NULL;
    probe.capacity = 0;
    probe.length = 0;
    CHECK_STATUS(NOOKERY_DATA_STATUS_BUFFER_TOO_SMALL, nookery_data_root(handle, &probe));
    size_t required = probe.length;
    CHECK(required == strlen(root) + 1);

    /* One byte short: nothing written, requirement repeated. */
    Out short_out;
    out_init(&short_out, required - 1);
    NookeryDataBuffer short_buffer = out_buffer(&short_out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_BUFFER_TOO_SMALL,
                 nookery_data_root(handle, &short_buffer));
    CHECK(short_buffer.length == required);
    CHECK(out_untouched(&short_out));

    /* Exact fit: success, NUL-terminated, length excludes the terminator. */
    Out exact;
    out_init(&exact, required);
    NookeryDataBuffer exact_buffer = out_buffer(&exact);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_root(handle, &exact_buffer));
    CHECK(exact_buffer.length == required - 1);
    CHECK(exact.bytes[required - 1] == '\0');
    CHECK(strcmp(exact.bytes, root) == 0);

    nookery_data_handle_destroy(handle);
}

static void test_path_escape(const char *root) {
    NookeryDataHandle *handle = make_handle(root, NULL);

    const char *escapes[] = { "../outside", "save/../../outside", "/etc/passwd", "" };
    for (size_t i = 0; i < sizeof escapes / sizeof escapes[0]; ++i) {
        Out out;
        out_init(&out, sizeof out.bytes);
        NookeryDataBuffer buffer = out_buffer(&out);
        CHECK_STATUS(NOOKERY_DATA_STATUS_PATH_REJECTED,
                     nookery_data_user_path(handle, escapes[i], &buffer));
        CHECK(buffer.length == 0);
        CHECK(out_untouched(&out));
        CHECK_STATUS(NOOKERY_DATA_STATUS_PATH_REJECTED, nookery_data_last_status(handle));
    }

    /* A legitimate relative path resolves below the root. */
    Out out;
    out_init(&out, sizeof out.bytes);
    NookeryDataBuffer buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK,
                 nookery_data_user_path(handle, "save/card_a/town.gci", &buffer));
    char expected[1024];
    CHECK(join_path(expected, sizeof expected, root, "save/card_a/town.gci"));
    CHECK(strcmp(out.bytes, expected) == 0);
    CHECK(buffer.length == strlen(expected));

    /* Base path: unavailable without a base root, joined with one. */
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_BASE_PATH_UNAVAILABLE,
                 nookery_data_base_path(handle, "texture_pack", &buffer));
    CHECK(out_untouched(&out));

    nookery_data_handle_destroy(handle);

    char base[1024];
    CHECK(append_suffix(base, sizeof base, root, "-base"));
    CHECK(make_dirs(base) == 0);
    NookeryDataHandle *based = make_handle(root, base);
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_base_path(based, "texture_pack", &buffer));
    CHECK(join_path(expected, sizeof expected, base, "texture_pack"));
    CHECK(strcmp(out.bytes, expected) == 0);

    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_PATH_REJECTED,
                 nookery_data_base_path(based, "../etc", &buffer));
    CHECK(out_untouched(&out));
    nookery_data_handle_destroy(based);
}

static void test_blocked_directory(const char *root) {
    /* A file where `save/card_a` must go: reported as NOT_DIRECTORY, the file is
     * preserved, and the remaining directories are still created. */
    char save[1024];
    char blocker[1024];
    CHECK(join_path(save, sizeof save, root, "save"));
    CHECK(join_path(blocker, sizeof blocker, root, "save/card_a"));
    CHECK(make_dirs(save) == 0);
    CHECK(write_file(blocker, "not a directory") == 0);

    NookeryDataHandle *handle = make_handle(root, NULL);
    NookeryDataDirReport report;
    report.struct_size = (uint32_t)sizeof report;
    report.failure_count = 0xffffffffu;
    CHECK_STATUS(NOOKERY_DATA_STATUS_NOT_DIRECTORY, nookery_data_initialize(handle, &report));
    CHECK(report.failure_count == 1);
    CHECK(file_equals(blocker, "not a directory"));

    char card_b[1024];
    char cache[1024];
    CHECK(join_path(card_b, sizeof card_b, root, "save/card_b"));
    CHECK(join_path(cache, sizeof cache, root, "cache"));
    CHECK(is_dir(card_b));
    CHECK(is_dir(cache));
    CHECK_STATUS(NOOKERY_DATA_STATUS_NOT_DIRECTORY, nookery_data_last_status(handle));

    /* The diagnostic names the failing path; the path text comes from the
     * library, not from this file. */
    Out message;
    out_init(&message, sizeof message.bytes);
    NookeryDataBuffer buffer = out_buffer(&message);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_last_message(handle, &buffer));
    CHECK(buffer.length > 0);
    CHECK(strstr(message.bytes, "card_a") != NULL);
    CHECK(strcmp(message.bytes, nookery_data_status_message(
                                   NOOKERY_DATA_STATUS_NOT_DIRECTORY)) != 0);
    /* Reading the diagnostic must not erase the status it reports. */
    CHECK_STATUS(NOOKERY_DATA_STATUS_NOT_DIRECTORY, nookery_data_last_status(handle));

    nookery_data_handle_destroy(handle);
}

static void test_migration_plan(const char *root) {
    char legacy[1024];
    CHECK(append_suffix(legacy, sizeof legacy, root, "-legacy"));
    CHECK(make_dirs(legacy) == 0);

    char path[1024];
    CHECK(join_path(path, sizeof path, legacy, "save"));
    CHECK(make_dirs(path) == 0);
    CHECK(join_path(path, sizeof path, legacy, "nes_roms"));
    CHECK(make_dirs(path) == 0);
    CHECK(join_path(path, sizeof path, legacy, "config.json"));
    CHECK(write_file(path, "{}") == 0);
    CHECK(join_path(path, sizeof path, legacy, "town.gci"));
    CHECK(write_file(path, "gci") == 0);
    CHECK(join_path(path, sizeof path, legacy, "notes.txt"));
    CHECK(write_file(path, "ignored") == 0);

    NookeryDataHandle *handle = make_handle(root, NULL);
    /* `save` already exists under the active root, so it must not be planned. */
    CHECK(make_dirs(root) == 0);
    CHECK(join_path(path, sizeof path, root, "save"));
    CHECK(make_dirs(path) == 0);

    NookeryDataPlan *plan = NULL;
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK,
                 nookery_data_plan_migration(handle, legacy, &plan));
    CHECK(plan != NULL);

    size_t directories = 0;
    size_t files = 0;
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK,
                 nookery_data_plan_counts(plan, &directories, &files));
    CHECK(directories == 1);
    CHECK(files == 2);

    Out out;
    out_init(&out, sizeof out.bytes);
    NookeryDataBuffer buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_plan_directory(plan, 0, &buffer));
    char expected[1024];
    CHECK(join_path(expected, sizeof expected, legacy, "nes_roms"));
    CHECK(strcmp(out.bytes, expected) == 0);

    CHECK(join_path(expected, sizeof expected, legacy, "config.json"));
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_plan_file(plan, 0, &buffer));
    CHECK(strcmp(out.bytes, expected) == 0);

    CHECK(join_path(expected, sizeof expected, legacy, "town.gci"));
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_OK, nookery_data_plan_file(plan, 1, &buffer));
    CHECK(strcmp(out.bytes, expected) == 0);

    /* Out-of-range and null inputs. */
    out_init(&out, sizeof out.bytes);
    buffer = out_buffer(&out);
    CHECK_STATUS(NOOKERY_DATA_STATUS_INDEX_OUT_OF_RANGE,
                 nookery_data_plan_file(plan, 2, &buffer));
    CHECK(out_untouched(&out));
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_plan_counts(plan, NULL, &files));

    /* Planning copies nothing. */
    char active_nes[1024];
    CHECK(join_path(active_nes, sizeof active_nes, root, "nes_roms"));
    CHECK(!is_dir(active_nes));

    nookery_data_plan_destroy(plan);
    nookery_data_plan_destroy(NULL);

    /* A rejected legacy root is reported, and the out-plan stays null. */
    plan = (NookeryDataPlan *)&plan;
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_plan_migration(handle, NULL, &plan));
    CHECK(plan == NULL);
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_plan_migration(handle, "relative", &plan));
    CHECK_STATUS(NOOKERY_DATA_STATUS_INVALID_ARGUMENT,
                 nookery_data_plan_migration(handle, legacy, NULL));

    nookery_data_handle_destroy(handle);
}

/* Remove everything this harness created, so repeated runs are deterministic. */
static void cleanup(const char *root) {
    char command[1400];
    int written = snprintf(command, sizeof command,
                           "rm -rf '%.300s' '%.300s-base' '%.300s-legacy'",
                           root, root, root);
    if (written < 0 || (size_t)written >= sizeof command || system(command) != 0) {
        fprintf(stderr, "WARN: cleanup of %s failed\n", root);
    }
}

int main(void) {
    char root[1024];
    snprintf(root, sizeof root, "/tmp/nookery-data-ffi-harness-%ld", (long)getpid());
    cleanup(root);
    if (make_dirs(root) != 0) {
        fprintf(stderr, "FAIL: could not create temp root %s\n", root);
        return 2;
    }

    test_identity();
    test_create_rejections(root);
    test_null_arguments(root);
    {
        /* Own root so the success case starts from nothing. */
        char fresh[1024];
        if (!append_suffix(fresh, sizeof fresh, root, "/fresh")) {
            CHECK(0 && "fresh root path too long");
            return 2;
        }
        test_success_and_repeat(fresh);
        cleanup(fresh);
    }
    test_missing_root(root);
    {
        char plain[1024];
        CHECK(join_path(plain, sizeof plain, root, "plain"));
        CHECK(make_dirs(plain) == 0);
        test_undersized_output(plain);
        test_path_escape(plain);
    }
    {
        char blocked[1024];
        CHECK(join_path(blocked, sizeof blocked, root, "blocked"));
        CHECK(make_dirs(blocked) == 0);
        test_blocked_directory(blocked);
    }
    {
        char plan_root[1024];
        CHECK(join_path(plan_root, sizeof plan_root, root, "plan"));
        CHECK(make_dirs(plan_root) == 0);
        test_migration_plan(plan_root);
    }

    cleanup(root);
    fprintf(stderr, "%d checks, %d failures\n", checks, failures);
    if (failures != 0) {
        fprintf(stderr, "HARNESS_RESULT=FAIL\n");
        return 1;
    }
    fprintf(stderr, "HARNESS_RESULT=PASS\n");
    return 0;
}
