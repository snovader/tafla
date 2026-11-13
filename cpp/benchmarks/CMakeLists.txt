# The comparison of tafla with other C++ libraries that encode tables, which
# README.md describes. It needs FlatBuffers, protobuf and msgpack-cxx, such as
# Homebrew's or a Linux distribution's packages.

find_package(flatbuffers CONFIG REQUIRED)
find_package(Protobuf CONFIG REQUIRED)
find_package(msgpack-cxx CONFIG REQUIRED)

set(TAFLA_GENERATED ${CMAKE_CURRENT_BINARY_DIR}/generated)
file(MAKE_DIRECTORY ${TAFLA_GENERATED})

# flatc's and protoc's code of the workloads, generated at build time, as
# those libraries' documentation builds it
add_custom_command(
    OUTPUT ${TAFLA_GENERATED}/workloads_generated.h
    COMMAND flatbuffers::flatc --cpp -o ${TAFLA_GENERATED} ${CMAKE_CURRENT_SOURCE_DIR}/schemas/workloads.fbs
    DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/schemas/workloads.fbs
    COMMENT "Generating FlatBuffers' code of the workloads")
add_custom_target(tafla_benchmarks_flatbuffers DEPENDS ${TAFLA_GENERATED}/workloads_generated.h)

add_library(tafla_benchmarks_protobuf STATIC ${CMAKE_CURRENT_SOURCE_DIR}/schemas/messages.proto)
target_link_libraries(tafla_benchmarks_protobuf PUBLIC protobuf::libprotobuf)
target_include_directories(tafla_benchmarks_protobuf PUBLIC ${TAFLA_GENERATED})
protobuf_generate(TARGET tafla_benchmarks_protobuf IMPORT_DIRS ${CMAKE_CURRENT_SOURCE_DIR}/schemas
                  PROTOC_OUT_DIR ${TAFLA_GENERATED})

add_library(tafla_benchmarks_common INTERFACE)
add_dependencies(tafla_benchmarks_common tafla_benchmarks_flatbuffers)
target_include_directories(tafla_benchmarks_common INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/src
                                                              ${CMAKE_CURRENT_SOURCE_DIR}/generated ${TAFLA_GENERATED})
target_compile_definitions(tafla_benchmarks_common INTERFACE MSGPACK_NO_BOOST)
target_link_libraries(tafla_benchmarks_common INTERFACE tafla::tafla tafla_warnings tafla_benchmarks_protobuf
                                                        flatbuffers::flatbuffers msgpack-cxx)

foreach(program comparison encoding allocation)
    add_executable(tafla_${program} src/${program}.cpp)
    target_link_libraries(tafla_${program} PRIVATE tafla_benchmarks_common)
endforeach()

if(TAFLA_BUILD_TESTS)
    add_executable(benchmarks_test tests/benchmarks_test.cpp)
    target_link_libraries(benchmarks_test PRIVATE tafla_benchmarks_common GTest::gtest_main)
    gtest_discover_tests(benchmarks_test DISCOVERY_TIMEOUT 60)
endif()
