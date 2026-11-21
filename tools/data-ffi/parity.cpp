/* parity.cpp — C++/Rust data-folder parity gate.
 *
 * This executable deliberately compiles the unchanged C++ data implementation
 * into a separate test target.  It is opt-in from CMake because the production
 * target must continue to use pc/src/acgc_data.cpp until this gate is green in
 * a configured host tree.
 */
#include "acgc/data.hpp"
#include "borealis/io.hpp"
#include "nookery_data.h"

#include <array>
#include <cstdlib>
#include <cstdint>
#include <cstdio>
#include <filesystem>
#include <string>
#include <utility>

#include <unistd.h>

namespace {

int checks = 0;
int failures = 0;

#define CHECK(condition)                                                        \
    do {                                                                       \
        ++checks;                                                              \
        if (!(condition)) {                                                    \
            ++failures;                                                        \
            std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #condition); \
        }                                                                      \
    } while (0)

std::string cpp_status_message(const borealis::data::Status& status) {
    using enum borealis::data::ErrorCode;

    std::string message;
    switch (status.code) {
        case None:
            return {};
        case NotInitialized:
            message = "Data folders have not been initialized.";
            break;
        case PreferencePathUnavailable:
            message = "The system data folder is unavailable.";
            break;
        case EmptyPath:
            message = "Choose a folder.";
            break;
        case CreateDirectoryFailed:
            message = "The selected folder could not be created.";
            break;
        case NotDirectory:
            message = "The selected path is not a folder.";
            break;
        case WriteProbeFailed:
            message = "The selected folder is not writable.";
            break;
        case WriteProbeCleanupFailed:
            message = "The selected folder could not be validated.";
            break;
        case DescriptorWriteFailed:
            message = "The data folder setting could not be saved.";
            break;
        case MigrationIncomplete:
            message = "Data migration could not be completed.";
            break;
        case OverrideActive:
            message = "The data folder is fixed by --user-dir for this session.";
            break;
        case Unsupported:
            message = "Changing the data folder is not supported on this platform.";
            break;
        case OpenFolderFailed:
            message = "The data folder could not be opened.";
            break;
    }

    if (!status.path.empty()) {
        message += " (" + borealis::io::fs_path_to_string(status.path) + ")";
    }
    if (status.systemError) {
        message += ": " + status.systemError.message();
    }
    return message;
}

std::string rust_status_message(uint32_t status) {
    const char* message = nookery_data_status_message(status);
    return message == nullptr ? std::string{} : std::string{ message };
}

std::string rust_path(NookeryDataHandle* handle, uint32_t kind) {
    std::array<char, 512> storage{};
    NookeryDataBuffer buffer{
        .struct_size = static_cast<uint32_t>(sizeof(NookeryDataBuffer)),
        .ptr = storage.data(),
        .capacity = storage.size(),
        .length = 0,
    };
    const auto status = nookery_data_directory(handle, kind, &buffer);
    CHECK(status == NOOKERY_DATA_STATUS_OK);
    if (status != NOOKERY_DATA_STATUS_OK || buffer.length >= buffer.capacity) {
        return {};
    }
    CHECK(storage[buffer.length] == '\0');
    return std::string{ storage.data() };
}

void remove_root(const std::filesystem::path& root) {
    std::error_code error;
    std::filesystem::remove_all(root, error);
    CHECK(!error);
}

} // namespace

int main() {
    const auto root = std::filesystem::temp_directory_path() /
                      ("nookery-data-ffi-parity-" + std::to_string(static_cast<long>(getpid())));
    const auto preference_root = root / "preference-root";
    const auto preference_root_text = preference_root.string();
    CHECK(setenv("XDG_DATA_HOME", preference_root_text.c_str(), 1) == 0);
    CHECK(root.is_absolute());
    CHECK(root.string().size() < 300);
    remove_root(root);

    const auto paths = acgc::data::initialize(root);
    const auto cpp_root = acgc::data::manager().active_data_path();
    CHECK(std::filesystem::is_directory(cpp_root));
    CHECK(paths.cachePath.string().size() < 512);

    const std::string root_text = borealis::io::fs_path_to_string(cpp_root);
    NookeryDataCreateOptions options{
        .struct_size = static_cast<uint32_t>(sizeof(NookeryDataCreateOptions)),
        .abi_version = NOOKERY_DATA_ABI_VERSION,
        .root_utf8 = root_text.c_str(),
        .base_root_utf8 = nullptr,
    };
    NookeryDataHandle* handle = nullptr;
    CHECK(nookery_data_handle_create(&options, &handle) == NOOKERY_DATA_STATUS_OK);
    CHECK(handle != nullptr);
    const std::string cache_text = borealis::io::fs_path_to_string(paths.cachePath);
    CHECK(nookery_data_set_cache_root(handle, cache_text.c_str()) == NOOKERY_DATA_STATUS_OK);

    NookeryDataDirReport report{
        .struct_size = static_cast<uint32_t>(sizeof(NookeryDataDirReport)),
        .failure_count = UINT32_MAX,
    };
    CHECK(nookery_data_initialize(handle, &report) == NOOKERY_DATA_STATUS_OK);
    CHECK(report.failure_count == 0);

    const std::array<std::filesystem::path, 5> cpp_directories{
        cpp_root / "save/card_a",
        cpp_root / "save/card_b",
        cpp_root / "texture_pack",
        cpp_root / "nes_roms",
        paths.cachePath,
    };
    for (uint32_t kind = 0; kind < cpp_directories.size(); ++kind) {
        CHECK(std::filesystem::is_directory(cpp_directories[kind]));
        const auto actual = rust_path(handle, kind);
        CHECK(actual == borealis::io::fs_path_to_string(cpp_directories[kind]));
        CHECK(std::filesystem::is_directory(std::filesystem::path{ actual }));
    }

    // initialize(root) intentionally exercises the upstream --user-dir override
    // path, so changing the location is refused before the empty-path
    // validator runs.  EmptyPath's standalone wording is covered by the
    // status-code table below; this call must match the live override result.
    const auto override_status = acgc::data::set_custom_data_path({});
    CHECK(static_cast<uint32_t>(override_status.code) == NOOKERY_DATA_STATUS_OVERRIDE_ACTIVE);
    CHECK(cpp_status_message(override_status) ==
          rust_status_message(NOOKERY_DATA_STATUS_OVERRIDE_ACTIVE));

    using ErrorCode = borealis::data::ErrorCode;
    const std::array<std::pair<ErrorCode, uint32_t>, 13> status_codes{{
        { ErrorCode::None, NOOKERY_DATA_STATUS_OK },
        { ErrorCode::NotInitialized, NOOKERY_DATA_STATUS_NOT_INITIALIZED },
        { ErrorCode::PreferencePathUnavailable, NOOKERY_DATA_STATUS_PREFERENCE_PATH_UNAVAILABLE },
        { ErrorCode::EmptyPath, NOOKERY_DATA_STATUS_EMPTY_PATH },
        { ErrorCode::CreateDirectoryFailed, NOOKERY_DATA_STATUS_CREATE_DIRECTORY_FAILED },
        { ErrorCode::NotDirectory, NOOKERY_DATA_STATUS_NOT_DIRECTORY },
        { ErrorCode::WriteProbeFailed, NOOKERY_DATA_STATUS_WRITE_PROBE_FAILED },
        { ErrorCode::WriteProbeCleanupFailed, NOOKERY_DATA_STATUS_WRITE_PROBE_CLEANUP_FAILED },
        { ErrorCode::DescriptorWriteFailed, NOOKERY_DATA_STATUS_DESCRIPTOR_WRITE_FAILED },
        { ErrorCode::MigrationIncomplete, NOOKERY_DATA_STATUS_MIGRATION_INCOMPLETE },
        { ErrorCode::OverrideActive, NOOKERY_DATA_STATUS_OVERRIDE_ACTIVE },
        { ErrorCode::Unsupported, NOOKERY_DATA_STATUS_UNSUPPORTED },
        { ErrorCode::OpenFolderFailed, NOOKERY_DATA_STATUS_OPEN_FOLDER_FAILED },
    }};
    for (const auto& [code, raw] : status_codes) {
        CHECK(static_cast<uint32_t>(code) == raw);
        borealis::data::Status status{ .code = code };
        CHECK(cpp_status_message(status) == rust_status_message(raw));
    }

    nookery_data_handle_destroy(handle);
    remove_root(root);
    std::fprintf(stderr, "%d checks, %d failures\n", checks, failures);
    if (failures != 0) {
        std::fprintf(stderr, "PARITY_RESULT=FAIL\n");
        return 1;
    }
    std::fprintf(stderr, "PARITY_RESULT=PASS\n");
    return 0;
}
