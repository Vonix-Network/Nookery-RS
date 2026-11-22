if (NOT DEFINED NOOKERY_RUNTIME_HOST_SOURCE)
    message(FATAL_ERROR "NOOKERY_RUNTIME_HOST_SOURCE is required")
endif ()

file(READ "${NOOKERY_RUNTIME_HOST_SOURCE}" host_source)
if (host_source MATCHES "ac_entry[ \\t\\r\\n]*\\(")
    message(FATAL_ERROR "legacy ac_entry() route is forbidden in the production host")
endif ()
if (host_source MATCHES "boot_main[ \\t\\r\\n]*\\(")
    message(FATAL_ERROR "legacy boot_main() route is forbidden in the production host")
endif ()

message(STATUS "production host route assertion passed: no legacy entry calls")
