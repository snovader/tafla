cmake_minimum_required(VERSION 3.25)

project(tafla VERSION 0.0.0 DESCRIPTION "Binary tables of scalars, strings and typed arrays" LANGUAGES CXX)

# tafla's own targets compile as ISO C++20 rather than GNU's dialect, CMake's
# default, which predefines macros such as `linux` and `unix`. A project that
# adds tafla chooses its own.
set(CMAKE_CXX_EXTENSIONS OFF)

# The runtime is header-only, and depends only on the standard library of
# C++20, so that the compiler inlines and specializes its reads and writes
# where they're called
add_library(tafla INTERFACE)
add_library(tafla::tafla ALIAS tafla)
target_include_directories(tafla INTERFACE $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
                                           $<INSTALL_INTERFACE:include>)
target_compile_features(tafla INTERFACE cxx_std_20)

option(TAFLA_BUILD_TESTS "Build the tests" ${PROJECT_IS_TOP_LEVEL})
option(TAFLA_BUILD_BENCHMARKS "Build the comparison benchmarks, which need FlatBuffers, protobuf and msgpack-cxx" OFF)
option(TAFLA_BUILD_FUZZERS "Build the libFuzzer targets, which need a Clang with libFuzzer" OFF)
option(TAFLA_WARNINGS_AS_ERRORS "Treat the warnings of tafla's own code as errors" OFF)
option(TAFLA_FORCE_BYTE_SWAP "Read and write arrays element by element, as a big-endian host does" OFF)
set(TAFLA_SANITIZERS "" CACHE STRING "Sanitizers to build the tests with, such as address,undefined")

# The warnings that tafla's own targets compile with, which its dependencies
# don't
add_library(tafla_warnings INTERFACE)
if(MSVC)
    target_compile_options(tafla_warnings INTERFACE /W4 $<$<BOOL:${TAFLA_WARNINGS_AS_ERRORS}>:/WX>)
else()
    target_compile_options(
        tafla_warnings
        INTERFACE -Wall
                  -Wextra
                  -Wpedantic
                  -Wconversion
                  -Wsign-conversion
                  -Wshadow
                  -Wnon-virtual-dtor
                  -Wold-style-cast
                  -Wcast-align
                  -Wnull-dereference
                  -Wdouble-promotion
                  -Wimplicit-fallthrough
                  $<$<BOOL:${TAFLA_WARNINGS_AS_ERRORS}>:-Werror>)
endif()

if(TAFLA_FORCE_BYTE_SWAP)
    target_compile_definitions(tafla INTERFACE TAFLA_FORCE_BYTE_SWAP)
endif()

# Fuzzers instrument every target for libFuzzer's coverage, and link its
# driver into the fuzz targets alone, whose main it is
if(TAFLA_BUILD_FUZZERS)
    set(TAFLA_BUILD_TESTS ON)
    add_compile_options(-fsanitize=fuzzer-no-link)
endif()

if(TAFLA_SANITIZERS)
    add_compile_options(-fsanitize=${TAFLA_SANITIZERS} -fno-omit-frame-pointer -fno-sanitize-recover=all)
    add_link_options(-fsanitize=${TAFLA_SANITIZERS})
endif()

if(TAFLA_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()

if(TAFLA_BUILD_BENCHMARKS)
    add_subdirectory(benchmarks)
endif()

if(TAFLA_BUILD_FUZZERS)
    add_subdirectory(fuzz)
endif()

# `cmake --install` installs the headers and a package, which
# `find_package(tafla)` finds, of the target `tafla::tafla`
include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

install(DIRECTORY include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
install(TARGETS tafla EXPORT taflaTargets)
install(
    EXPORT taflaTargets
    NAMESPACE tafla::
    FILE taflaConfig.cmake
    DESTINATION ${CMAKE_INSTALL_DATADIR}/cmake/tafla)

# The headers are the same on every platform, so any architecture's build
# may use them
write_basic_package_version_file(${CMAKE_CURRENT_BINARY_DIR}/taflaConfigVersion.cmake COMPATIBILITY SameMinorVersion
                                 ARCH_INDEPENDENT)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/taflaConfigVersion.cmake DESTINATION ${CMAKE_INSTALL_DATADIR}/cmake/tafla)
