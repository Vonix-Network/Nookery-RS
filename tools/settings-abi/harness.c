#include "pc_settings.h"
#include "nookery_config.h"

#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <string.h>
#include <unistd.h>

static int checks;
static int failures;
PCSettings g_pc_settings;

#define CHECK(condition) do { \
    ++checks; \
    if (!(condition)) { \
        ++failures; \
        fprintf(stderr, "FAIL:%s:%d: %s\n", __FILE__, __LINE__, #condition); \
    } \
} while (0)

static int write_text(const char *path, const char *text) {
    FILE *file = fopen(path, "wb");
    int result;
    if (file == NULL) return 0;
    const size_t length = strlen(text);
    result = fwrite(text, 1, length, file) == length;
    if (fclose(file) != 0) result = 0;
    return result;
}

static char *read_text(const char *path) {
    FILE *file = fopen(path, "rb");
    long length;
    char *text;
    size_t read_length;
    if (file == NULL || fseek(file, 0, SEEK_END) != 0 || (length = ftell(file)) < 0 ||
        fseek(file, 0, SEEK_SET) != 0) {
        if (file != NULL) fclose(file);
        return NULL;
    }
    text = (char *)malloc((size_t)length + 1);
    if (text == NULL) {
        fclose(file);
        return NULL;
    }
    read_length = fread(text, 1, (size_t)length, file);
    fclose(file);
    if (read_length != (size_t)length) {
        free(text);
        return NULL;
    }
    text[length] = '\0';
    return text;
}

static int cleanup(const char *root) {
    char command[320];
    const int written = snprintf(command, sizeof command, "rm -rf '%s'", root);
    return written > 0 && (size_t)written < sizeof command && system(command) == 0;
}

static void test_layout(void) {
    CHECK(sizeof(int) == 4);
    CHECK(sizeof(PCSettings) == 64);
    CHECK(_Alignof(PCSettings) == _Alignof(int));
    CHECK(offsetof(PCSettings, window_width) == 0);
    CHECK(offsetof(PCSettings, window_height) == 4);
    CHECK(offsetof(PCSettings, fullscreen) == 8);
    CHECK(offsetof(PCSettings, vsync) == 12);
    CHECK(offsetof(PCSettings, max_fps) == 16);
    CHECK(offsetof(PCSettings, preload_textures) == 20);
    CHECK(offsetof(PCSettings, disable_resetti) == 24);
    CHECK(offsetof(PCSettings, disable_shop_visitor_req) == 28);
    CHECK(offsetof(PCSettings, borderless_acres) == 32);
    CHECK(offsetof(PCSettings, nes_aspect) == 36);
    CHECK(offsetof(PCSettings, master_volume) == 40);
    CHECK(offsetof(PCSettings, stick_deadzone) == 44);
    CHECK(offsetof(PCSettings, cstick_deadzone) == 48);
    CHECK(offsetof(PCSettings, fast_text) == 52);
    CHECK(offsetof(PCSettings, travel_move_out_chance) == 56);
    CHECK(offsetof(PCSettings, hud_at_edges) == 60);
    CHECK(nookery_settings_abi_version() == NOOKERY_SETTINGS_ABI_VERSION);
    CHECK(nookery_settings_status_count() == NOOKERY_SETTINGS_STATUS_MAX + 1);
    CHECK(strcmp(nookery_settings_status_name(NOOKERY_SETTINGS_STATUS_OK), "OK") == 0);
}

static void test_coexistence(void) {
    g_pc_settings.window_width = -1;
    g_pc_settings.window_height = 20000;
    CHECK(nookery_settings_apply(&g_pc_settings) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(g_pc_settings.window_width == 320 && g_pc_settings.window_height == 16384);
}

static void test_defaults_and_clamps(const char *path) {
    NookerySettingsHandle *handle = NULL;
    PCSettings settings;
    CHECK(nookery_settings_create(path, &handle) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(handle != NULL);
    CHECK(nookery_settings_load(handle, &settings) == NOOKERY_SETTINGS_STATUS_FILE_MISSING);
    CHECK(settings.window_width == 1280 && settings.window_height == 960 && settings.fullscreen == 0);
    CHECK(settings.vsync == 1 && settings.max_fps == 60 && settings.preload_textures == 0);
    CHECK(settings.disable_resetti == 0 && settings.disable_shop_visitor_req == 0);
    CHECK(settings.borderless_acres == 1 && settings.nes_aspect == 1 && settings.fast_text == 1);
    CHECK(settings.master_volume == 100 && settings.stick_deadzone == 12 && settings.cstick_deadzone == 12);
    CHECK(settings.travel_move_out_chance == 100 && settings.hud_at_edges == 0);

    settings.window_width = -1;
    settings.window_height = 20000;
    settings.fullscreen = 99;
    settings.vsync = -4;
    settings.max_fps = 2001;
    settings.preload_textures = -1;
    settings.master_volume = -1;
    settings.stick_deadzone = 100;
    settings.cstick_deadzone = -3;
    settings.travel_move_out_chance = 25;
    CHECK(nookery_settings_apply(&settings) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(settings.window_width == 320 && settings.window_height == 16384 && settings.fullscreen == 2);
    CHECK(settings.vsync == 1 && settings.max_fps == 1000 && settings.preload_textures == 0);
    CHECK(settings.master_volume == 0 && settings.stick_deadzone == 40 && settings.cstick_deadzone == 0);
    CHECK(settings.travel_move_out_chance == 100);
    CHECK(nookery_settings_save(handle, &settings) == NOOKERY_SETTINGS_STATUS_OK);
    nookery_settings_destroy(handle);
}

static void test_file_codec(const char *path) {
    const char *input = "{\"future\":{\"nested\":[true,3.5]},\"video.windowWidth\":99999,"
                        "\"video.windowHeight\":479,\"video.uiScale\":-5,"
                        "\"audio.masterVolume\":\"loud\",\"video.renderer\":\"vulkan\"}";
    NookerySettingsHandle *handle = NULL;
    PCSettings settings;
    char *first;
    char *second;
    CHECK(write_text(path, input));
    CHECK(nookery_settings_create(path, &handle) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(nookery_settings_load(handle, &settings) == NOOKERY_SETTINGS_STATUS_WRONG_TYPE);
    CHECK(settings.window_width == 16384 && settings.window_height == 479);
    CHECK(settings.master_volume == 100 && settings.vsync == 1);
    CHECK(nookery_settings_save(handle, &settings) == NOOKERY_SETTINGS_STATUS_OK);
    first = read_text(path);
    CHECK(first != NULL);
    CHECK(first != NULL && strstr(first, "\"future\":{\"nested\":[true,3.5]}") != NULL);
    CHECK(first != NULL && strstr(first, "\"video.renderer\":\"auto\"") != NULL);
    CHECK(nookery_settings_load(handle, &settings) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(nookery_settings_save(handle, &settings) == NOOKERY_SETTINGS_STATUS_OK);
    second = read_text(path);
    CHECK(first != NULL && second != NULL && strcmp(first, second) == 0);
    free(first);
    free(second);
    nookery_settings_destroy(handle);
}

static void test_file_errors(const char *empty_path, const char *bad_path) {
    NookerySettingsHandle *handle = NULL;
    PCSettings settings = { 0 };
    CHECK(write_text(empty_path, "\n  \t"));
    CHECK(nookery_settings_create(empty_path, &handle) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(nookery_settings_load(handle, &settings) == NOOKERY_SETTINGS_STATUS_EMPTY_FILE);
    CHECK(settings.window_width == 1280);
    nookery_settings_destroy(handle);
    handle = NULL;
    CHECK(write_text(bad_path, "{broken"));
    CHECK(nookery_settings_create(bad_path, &handle) == NOOKERY_SETTINGS_STATUS_OK);
    settings.window_width = 777;
    CHECK(nookery_settings_load(handle, &settings) == NOOKERY_SETTINGS_STATUS_MALFORMED_JSON);
    CHECK(settings.window_width == 777);
    nookery_settings_destroy(handle);
}

static void test_resolution_and_arguments(const char *path) {
    NookerySettingsHandle *handle = NULL;
    int width = 640;
    int height = 480;
    CHECK(nookery_settings_cycle_resolution(&width, &height, 1) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(width == 800 && height == 600);
    CHECK(nookery_settings_cycle_resolution(&width, &height, -1) == NOOKERY_SETTINGS_STATUS_OK);
    CHECK(width == 640 && height == 480);
    CHECK(nookery_settings_cycle_resolution(&width, &height, 0) == NOOKERY_SETTINGS_STATUS_INVALID_DIRECTION);
    CHECK(nookery_settings_create(NULL, &handle) == NOOKERY_SETTINGS_STATUS_INVALID_ARGUMENT);
    CHECK(handle == NULL);
    CHECK(nookery_settings_create(path, NULL) == NOOKERY_SETTINGS_STATUS_INVALID_ARGUMENT);
    CHECK(nookery_settings_load(NULL, NULL) == NOOKERY_SETTINGS_STATUS_INVALID_ARGUMENT);
    CHECK(nookery_settings_apply(NULL) == NOOKERY_SETTINGS_STATUS_INVALID_ARGUMENT);
    nookery_settings_destroy(NULL);
}

int main(void) {
    char root[256];
    char missing[300];
    char json_path[300];
    char empty_path[300];
    char bad_path[300];
    snprintf(root, sizeof root, "/tmp/nookery-settings-abi-%ld", (long)getpid());
    snprintf(missing, sizeof missing, "%s/missing.json", root);
    snprintf(json_path, sizeof json_path, "%s/config.json", root);
    snprintf(empty_path, sizeof empty_path, "%s/empty.json", root);
    snprintf(bad_path, sizeof bad_path, "%s/bad.json", root);
    if (!cleanup(root)) return 2;
    CHECK(mkdir(root, 0700) == 0);
    test_layout();
    test_coexistence();
    test_defaults_and_clamps(missing);
    test_file_codec(json_path);
    test_file_errors(empty_path, bad_path);
    test_resolution_and_arguments(missing);
    if (!cleanup(root)) return 2;
    printf("EXPECTED_CHECKS=70\nCHECKS=%d\nFAILURES=%d\n", checks, failures);
    printf("HARNESS_RESULT=%s\n", failures == 0 && checks == 70 ? "PASS" : "FAIL");
    return failures == 0 && checks == 70 ? 0 : 1;
}
