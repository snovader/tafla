include(FetchContent)

# Test-only dependencies, pinned, and built from source so that the tests
# build alike on every platform and under the sanitizers
FetchContent_Declare(
    googletest
    GIT_REPOSITORY https://github.com/google/googletest.git
    GIT_TAG 52eb8108c5bdec04579160ae17225d66034bd723 # v1.17.0
    GIT_SHALLOW FALSE
    EXCLUDE_FROM_ALL SYSTEM)
FetchContent_Declare(
    rapidcheck
    GIT_REPOSITORY https://github.com/emil-e/rapidcheck.git
    GIT_TAG 2c3c4365aca21ef4e612768fdc95b1ce8b39a651
    EXCLUDE_FROM_ALL SYSTEM)
FetchContent_Declare(
    nlohmann_json
    URL https://github.com/nlohmann/json/releases/download/v3.12.0/json.tar.xz
    URL_HASH SHA256=42f6e95cad6ec532fd372391373363b62a14af6d771056dbfc86160e6dfff7aa
    EXCLUDE_FROM_ALL SYSTEM)

set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
set(RC_ENABLE_GTEST ON CACHE BOOL "" FORCE)
set(JSON_BuildTests OFF CACHE INTERNAL "")
FetchContent_MakeAvailable(googletest rapidcheck nlohmann_json)

include(GoogleTest)

# The descriptions of tables that every test builds, writes and reads
add_library(
    tafla_test_common STATIC
    common/build.cpp
    common/cases.cpp
    common/description.cpp
    common/read.cpp
    common/strategy.cpp
    common/write.cpp)
target_include_directories(tafla_test_common PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(tafla_test_common PUBLIC tafla::tafla tafla_warnings rapidcheck rapidcheck_gtest GTest::gtest
                                               nlohmann_json::nlohmann_json)
target_compile_definitions(tafla_test_common PUBLIC TAFLA_CONFORMANCE="${PROJECT_SOURCE_DIR}/../conformance")

set(TAFLA_TESTS
    array_view_test
    byte_buffer_test
    compatibility_test
    conformance_test
    decode_error_test
    exception_safety_test
    format_test
    layout_test
    round_trip_test
    table_builder_test
    table_writer_test
    utf8_test)

foreach(test ${TAFLA_TESTS})
    add_executable(${test} ${test}.cpp)
    target_link_libraries(${test} PRIVATE tafla_test_common GTest::gtest_main)
    # proptest's 256 cases of each property, rather than RapidCheck's 100
    gtest_discover_tests(${test} DISCOVERY_TIMEOUT 60 PROPERTIES ENVIRONMENT "RC_PARAMS=max_success=256")
endforeach()

add_subdirectory(generated)
